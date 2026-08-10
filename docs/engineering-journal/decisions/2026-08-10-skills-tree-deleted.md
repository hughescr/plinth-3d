# Decision — the skills/ tree is deleted

**Ruling given 2026-08-09 by the owner (Craig Hughes).** The entire `skills/` tree
is deleted, in full, with no retained files.

## What was deleted

All four skill directories and everything under them:

- `skills/prep-mini/` — `SKILL.md` plus `references/{hollowing,repair-playbook,sizing}.md`
- `skills/decimate-for-print/` — `SKILL.md`
- `skills/support-placement/` — `SKILL.md` plus
  `references/{design,design-v4,research-tree-supports,tree-construction-literature,v4-kickoff-prompt}.md`
- `skills/chitubox-support-injection/` — `SKILL.md` plus
  `references/{chitubox-format,magic-support-profile}.md`

## What this overturns

`04-migration-plan.md:1805` recorded a retain ruling: the `SKILL.md` files would
be marked superseded, but `skills/prep-mini/references/*.md` and
`skills/support-placement/references/*.md` would stay in place unrewritten,
"as historical reasoning" — kept specifically because the acceptance grep in that
same plan (`04-migration-plan.md:1821`) excluded `skills/` from the
"no more `.py` mentions" check on the grounds that those reference docs
legitimately still describe the retired Python workflow. This ruling overturns
that retention: nothing in `skills/` survives, reference docs included.

## Why

The skills instructed agents to run Python (Blender-MCP driven `decimate_pipeline.py`,
`geometry_sweep.py`, `hollow_drill.py`, `chitubox_inject.py`, and friends) that no
longer exists — it was deleted at c8f0801 ("E8(scripts): delete the pytest harness,
the legacy pipeline, and the frozen engine", 2026-08-08). A skill that dispatches to
a deleted interpreter and deleted scripts is not historical reasoning, it is a live
trap for the next agent that reads it.

The one item out of that tree with load-bearing content — the units contract
(source-coordinates-are-final-printed-millimetres, and what a slicer rescale
silently invalidates) — was already carried forward into the Rust docs by the
hygiene sweep landed the same day as this ruling (commit `0d16d51`, 2026-08-10:
"the units contract gets said out loud," landing on `plinth-decimate`'s module doc
and `PrepareRequest`). Nothing else in the tree had a live reader; the algorithmic
and physics reasoning it once held is already superseded by the ported Rust
implementations and the ratified specs under `docs/engineering-journal/specs/`.

## Recovery path

Git history. The tree is recoverable in full from the commit immediately before
this deletion lands — `git show <that-commit>^:skills` (or `git log -- skills/`
from any later commit) reaches every file. No working-tree copy is kept.
