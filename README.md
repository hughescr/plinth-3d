# resin-print-prep

Two skills for making 3D meshes resin-printable in Blender, driven through
the [Blender MCP](https://extensions.blender.org/add-ons/mcp/):

- **prep-mini** — the full fix-anything workflow: inspect → orient → union
  shells (voxel remesh, never exact boolean) → converging geometry-repair
  sweep → scale/center baked to tabletop mm → error-bounded decimation →
  verified STL export.
- **decimate-for-print** — standalone density reduction: derives an error
  budget from your printer's pixel pitch (deviation ≤ half a pixel),
  protects thin features that quadric collapse would destroy, bisects for
  the deepest passing reduction, and verifies against the full original
  surface before exporting. Batch-friendly (headless Blender per model).

New in 1.1: `scripts/tools/hollow_drill.py` — supervised hollowing (EDT-based
hollow/solid decision, main-cavity-only, per-lobe drains, pooling audit,
vent, verified export) plus print-orientation search. Pure Python
(trimesh/scipy/scikit-image/manifold3d), no Blender required.

New in 1.3: `chitubox-support-injection` skill — write native, editable
supports (and raft bridges) directly into CHITUBOX project files via a
reverse-engineered, injection-validated format map
(`scripts/support_placement/chitubox_inject.py` + full format reference).

New in 1.4: `support-placement` v2 — the placement companion is no
longer planned, it's implemented: engineered tip/shaft sizing from
green-state resin mechanics (not fixed palettes), scar-cost-aware
placement, articulated/braced routing, clearance-envelope self-
verification, and from-scratch synthesized-mesh emission via the
sibling injection skill. `scripts/support_placement/` (package, pytest
suite in `scripts/tests/`). Validated end-to-end on a real 576k-face
model (`skills/support-placement/references/design.md` has the full
before/after and honestly-reported known limitations).

Shared machinery in `scripts/tools/`: `decimate_pipeline.py` (also runs from the
CLI: `blender --background host.blend --python decimate_pipeline.py -- --stl model.stl`)
and `geometry_sweep.py` (repair loop + scale/center bake).

Real-world result: a 13-model minis folder went 13.3M → 1.37M triangles
(9.7×), every model verified watertight with p99 surface deviation under
half a printer pixel (9.5 µm on a Saturn 4 Ultra 16K).

Requires Blender 4.2+ with the Blender MCP add-on; numpy (bundled with
Blender). Printer defaults assume Elegoo Saturn 4 Ultra 16K (14×19 µm) —
pass your own pixel pitch.
