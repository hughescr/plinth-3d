# Calibration plates 2 and 3 — designed, never printed

**There are no results for plates 2 and 3 because neither plate was ever
printed.** This file exists so that the next person to go looking stops here
instead of searching git history and chat transcripts, which is what it cost to
establish this on 2026-08-30.

---

## The evidence

**Print order was 1 → 4 → 5 → 6.** Plates 2 and 3 were designed in the same pass
as plate 4, after plate 1 bounded the pressure envelope and all three were
re-centred on it. Only plate 4 was built.

Three independent facts fix this:

1. **Film age.** Reading plate 4, the owner recorded: *"We have done exactly 2
   prints now on this PFA film; it's still practically brand new."* Those two
   prints are plate 1 and plate 4.
2. **The owner's own words, immediately after the plate-4 read:** *"Let's plan
   the next plate then - I think it'd be good to get some more info on 'bridge'
   type shelves, but I'd also like to **start measuring the stuff plate 3 is
   designed to test**."* You do not plan to start measuring what a plate tests if
   that plate has been printed.
3. **What was printed next.** The plate that followed was **plate 5**
   (compliant vs stiff) — families S (splayed bridges), TIP and LAD. Bridges and
   tips: exactly the two things named in that request. Plate 3's subject matter
   was folded into plate 5's design rather than printed as plate 3.

**No results file, no scored specimen, and no photograph of a plate-2 or plate-3
part exists** in this repository, in any commit on any branch including deleted
files, in the constants source ledger, or in any retained session transcript.
Searches for their specimen ID families (`B**`, `F[ELF]nn`, `WN`, `E2*` for plate
2; `C**`, `CL*`, `CH*`, `CS*`, `WT`, `E3*` for plate 3) return only design
discussion.

## What still exists

The **designs** are complete and survive in full:

- [`plate-generator-PLATE-LAYOUT.md`](../../evidence/plate-generator-PLATE-LAYOUT.md)
  — pre-registered purpose, specimen tables, validity gates, train/ballast
  structure and scoring sheets for all six plates.
- [`plate-generator-manifest.json`](../../evidence/plate-generator-manifest.json)
  — specimen geometry.

What plate 2 was for: **family B, the necked cantilever** — the owner's actual
observed defect — plus **family F, cycle count at constant elapsed time**.

What plate 3 was for: strength of varying cross-section — families `C`, `CL`,
`CH`, `CS` — plus witness `WT`.

## What it would cost to print them now

**Not zero, and more than the design documents imply.** The generator that built
every plate (`calibration-coupons/`, Python) was deliberately deleted on
2026-07-31, once plate 6 was scored, per the owner's standing decision that it
was condemned. Python is now barred by a build guard. There is no coupon
generator in the Rust tree and no plate STL in the repository.

So printing plate 2 or 3 today means **regenerating the geometry from the
manifest with a new Rust generator**, then printing, then scoring. The design
work is done and does not need repeating; the toolchain does.

**Before assuming that:** check whether the sliced `.chitubox` build files for
plates 2 and 3 survive outside this repository. Plate 5's provenance records a
build md5 for its `.chitubox` file, so these artefacts existed. If plate 2's and
plate 3's survive on disk, they can go straight to the printer and the cost drops
to one printer-day each.

## Bearing on the load-model coupon programme

The load-model design requests a coupon **C1** described as *"score the two
purpose-built, never-scored existing plates"*, costed at *"~zero if the plates
survive"*.

**That description most likely refers to plates 2 and 3** — purpose-built,
designed, never scored. If so the costing is wrong in the way described above:
"never scored" is true because they were never *printed*, not because printed
parts went unread. Nothing is sitting on a shelf waiting to be photographed.

C1's actual question is plate **position** severity, which the design carries as a
bare 1.0 placeholder. Neither plate 2 nor plate 3 was built to answer that —
**plate 1 was**, and it returned one unexplained rear-station failure with a named
confound. See [`plate-01-results.md`](plate-01-results.md) §6, and the cheap
discriminating test recorded there: a strip of specimens at one Y spread across
X. That is much less than a plate, and it is the thing actually worth printing.
