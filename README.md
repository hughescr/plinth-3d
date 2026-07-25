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

New in 1.1: supervised hollowing (EDT-based hollow/solid decision,
main-cavity-only, per-lobe drains, pooling audit, vent, verified export) plus
print-orientation search. Pure Python (trimesh/scipy/scikit-image/manifold3d),
no Blender required. The algorithm lives in
`scripts/support_placement/hollow.py`; `scripts/tools/hollow_drill.py` is its
CLI (`in.stl out.stl`, exit 0 = written — hollowed or legitimately solid,
1 = bad input, 2 = self-verification failed and nothing was written, with the
failed checks named on stderr).

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

New in 1.5: the **emit path** is a library, not a hand-driven script.
`scripts/support_placement/emit.py` turns an `engine.run` result plus the STL
it was computed for into a `.chitubox`, built from an **empty** template by the
`chitubox_io` Rust crate (`rust/chitubox_io`, PyO3 binding, batch-first
columnar boundary). The old clone-a-base-project writer could staple correct
supports onto the wrong model — silently, in a file that parsed and verified;
the new path has no template parameter at all, so that cannot happen.
Operator entry point:
`python scripts/tools/emit_supports.py <model.stl> <out.chitubox>` — one JSON
report line on stdout, progress on stderr, distinct exit codes per failure
class.

New in 1.6: the **whole pipeline** is one call.
`scripts/support_placement/pipeline.py` composes hollow/drill → support engine
→ emit, with the orientation search on by default (orientation must be final
before drilling — drain placement is gravity-dependent). Every stage's own
report is captured, and every run writes a machine-readable record —
`<out stem>.pipeline.json`, on failure too — alongside the
`<out stem>.prepped.stl` the supports were actually computed against. A stage
that declines to produce output yields a non-zero exit code *and* that record;
"succeeded, wrote nothing" is not a reachable state. Operator entry point:

    python scripts/tools/prep_one.py <model.stl> <out.chitubox>
    python scripts/tools/prep_one.py --out-dir DIR <model.stl|dir>... --skip-existing

One JSON line per input on stdout whatever the outcome; `--skip-existing`
makes re-running a folder cheap and idempotent. Exit codes: 0 written/skipped,
1 bad input, 2 hollow/drill failed, 3 support gates failed, 4 emit failed,
5 support engine raised.

Both Rust extensions are built with `bash scripts/build_kernel.sh` (no
argument builds `geom_kernel` and `chitubox_io`; pass a crate name for one).

Shared machinery in `scripts/tools/`: `decimate_pipeline.py` (also runs from the
CLI: `blender --background host.blend --python decimate_pipeline.py -- --stl model.stl`)
and `geometry_sweep.py` (repair loop + scale/center bake).

Real-world result: a 13-model minis folder went 13.3M → 1.37M triangles
(9.7×), every model verified watertight with p99 surface deviation under
half a printer pixel (9.5 µm on a Saturn 4 Ultra 16K).

Requires Blender 4.2+ with the Blender MCP add-on; numpy (bundled with
Blender). Printer defaults assume Elegoo Saturn 4 Ultra 16K (14×19 µm) —
pass your own pixel pitch.
