# Print Audit — Addendum: Mesh Classification of Unreachable Contacts (final)

*Completes TREANT-V4-PRINT-AUDIT.md. Every action-changing call below was independently
re-verified by a second agent with its own ray-casting code; all four verdicts CONFIRMED.*

## The answer to "what are the 195 unreachable contacts, really?"

| Class | Count | Meaning |
|---|---|---|
| INTERNAL (phantom) | 78 | Buried inside the solid — the STL is a non-watertight assembly of overlapping shapes, and the engine flagged down-facing surfaces that are interior to the union. They print as solid resin. Never at risk. |
| GROOVE-BLOCKED | 94 | Real exterior spots where any support would collide with immediately-adjacent model (needed clearance ≈ 0.95 mm; measured gaps often < 0.5 mm). Mostly tiny specks already held by a neighboring support. |
| OPEN-BUT-UNREACHED | 23 | Supportable spots routing simply didn't reach — the genuine routing-reach work. Only 13 have no support within 2 mm. |

**Consequence: ~40% of "unrouted" and a third of "unsupported minima" were phantoms.**
The engine's real coverage gap is much smaller than the raw residual counts implied.

## Final manual-support list for ChituBox (verified)

Coordinates are the STL's native (model) coordinates — the file is already centered, so x/y
match directly; for height, `z + 64.0` = mm above the model's lowest point.

1. **(32.1, −12.1, 43.7)** — stranded thin overhang, ~10.2 mm from the nearest support.
   **Fine tip required**: the descent corridor measures only 0.25–0.51 mm of lateral
   clearance, so approach at an angle with the finest tip that will hold.
2. **(−19.5, 13.2, −11.7)** — broad down-facing overhang, ~83 mm², straight-down corridor
   open all the way to the plate, nothing within 3.7 mm. The most important addition.
3. **(11.6, −19, 32.2)** — reach gap, unheld by ~4 mm. Standard support.
4. **(3.8, −6.9, −10.2)** — smaller horizontal overhang (~36 mm², 5 mm of clear void below,
   no support in the downward cone). Weakest-evidenced of the four but confirmed; add one.

**Retracted:** the crown-tip cluster at (36.2, 3.9, 52.7) from the original report — verified
interior at 0/2000 ray escapes across three probe densities and crop sizes. No support needed
(or possible).

## Engine follow-ups this adds (beyond the main report's list)

- **Solid-union / visibility filter in perception** — stop demanding supports for faces
  interior to the merged solid. Shrinks the residual gates dramatically for free, and is
  essential for the mini queue (skeletons and multi-part sculpts are non-watertight
  assemblies by default).
- **Mesh repair / boolean union before slicing** recommended for this STL and as a standard
  pipeline pre-pass.

## Bottom line

Print-risk verdict stands at **LOW** — checklist: SUNLU resin, ~60 mm/min lift, the four
manual supports above. The phantoms cost nothing at print time (they're solid resin), they
only inflated the engine's residual counts.
