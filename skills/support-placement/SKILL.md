---
name: support-placement
description: "v2 engine (implemented + tested, real-model-validated): compute resin-print supports algorithmically - engineered sizing from cured-resin mechanics instead of fixed palettes, scar-cost-aware placement, articulated/braced routing, self-verification - emitting natively editable Chitubox supports directly (from-scratch synthesized segment meshes). Use when generating supports outside a slicer, when iterating this generator, or when reasoning about support mechanics (peel loads, green-state resin, slenderness/bracing)."
---

# Support Placement (v2)

Engine: `${CLAUDE_PLUGIN_ROOT}/scripts/support_placement/` (package;
see its `__init__.py` for the module map) with a CLI-free API driven via
`engine.run(mesh, cfg)`. Tests: `${CLAUDE_PLUGIN_ROOT}/scripts/tests/`
(pytest, 51 tests covering every module on synthetic fixtures, plus a
regression test that cross-validates the corpus-reading code against
`supported-treant.chitubox`'s independently-documented figures).

Full design charter, mechanics framework, v2 implementation notes, and
an honest real-world validation run (Weeping Treant v1.2.stl, 576k
faces) with known follow-ups: `references/design.md`.

Emission is now IN this skill (`chitubox_emit.py`: from-scratch
capped-frustum segment mesh synthesis + injection), reusing the sibling
`chitubox-support-injection` skill's `Project` parser/writer
(`chitubox_inject.py`) for the container format itself - pads/bridges
are still clone-based (only path validated for those two record types);
tip/taper/shaft/flare/joiner are synthesized from scratch with
continuous, off-palette radii. Never ship supports as bare STL - the
review artifact is a `.chitubox` the user opens normally, and injection
is append-only (never overwrites the user's existing supports).

v0's prototype (`support_gen_proto.py`, islands/density/routing/
self-verify, correct but unengineered) is superseded; its behavior is
preserved only as the historical baseline design.md's v0-vs-v2 table
compares against.
