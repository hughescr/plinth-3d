# Reference — durable written findings, indexed

Everything we worked out, wrote down, and still believe. **Not normative:**
`docs/specs/` holds the rules we ratified and that code cites as law; this
directory holds the measurements and the reasoning several of those specs were
built on.

This README indexes **every file** in this directory. If you are looking for a
number, a measurement, or the reason a constant has the value it has, it is
listed below — read the one line, then the file. Keep this index complete: a
new file here without an entry is a file nobody will find.

**What goes where.** `docs/specs/` = law. `docs/datasheets/` = vendor and
hardware data somebody else published (see
[`../datasheets/README.md`](../datasheets/README.md)). **Here** = our own
findings. Run outputs — result JSONs, censuses, CPU profiles, determinism/parity
comparisons, screenshots — are **not in git** at all; they are archived outside
the repo.

---

## Measured calibration-plate results

The physical plates. These are where the engine's load constants come from, and
the plates' own working folder no longer exists — **these files are the only
copy of that data.**

| file | what it is | read it when |
|---|---|---|
| [`plate-06-results.md`](plate-06-results.md) | **Record of record for calibration plate 6** (support-arm capacity; printed 2026-07-30, scored 2026-07-31). Full 34-specimen table with geometry and outcomes; load sharing is at least additive; cylindrical `X* ∈ (19.91, 30.48)`; family A6 void; the pre-registration override; the coupon-design rules that bind every future plate. | You need any measured support-load number, or you are designing another calibration plate. |
| [`plate-06-verification-physicist.md`](plate-06-verification-physicist.md) | Appendix — failure-mode physics memo from the 2026-07-31 adversarial panel. Where the cap-plane-z confound was first identified. | You want to re-open the A6 anomaly. |
| [`plate-06-verification-protocol.md`](plate-06-verification-protocol.md) | Appendix — protocol memo: build provenance, gate-by-gate validity ruling, and the literal plate-wide reading of the void. | You want to check whether a plate-6 conclusion was admissible. |
| [`plate-06-verification-refutation.json`](plate-06-verification-refutation.json) | Appendix — the refuter's per-claim verdicts and reasoning. Every caveat in the results file traces back to a verdict here. Two `//` provenance lines at the top make it JSONC: `tail -n +3` before `jq`. | You are about to quote a plate-6 finding without its caveats. |
| [`plate-05-results.md`](plate-05-results.md) | **Salvage record for calibration plate 5** (compliant vs stiff). Geometry for all 72 specimens; the conical `X* = [25.00, 25.34]` boundary, the area law, the narrowest-section failure mode. Outcomes are only **partly** recovered and every gap is marked. | You need the cap-area evidence that `X` is a ratio rather than a total-area threshold, or you are trying to recover the missing reads. |

## Literature

[`literature/`](literature/) — the peel-force and green-state literature, our
notes on it, and the source papers, kept together in one directory. Moved here
from `docs/datasheets/physics/` on 2026-07-31: it is a review we wrote, not a
vendor document.

| file | what it is | read it when |
|---|---|---|
| [`literature/LITERATURE-NOTES.md`](literature/LITERATURE-NOTES.md) | Primary-source measurements with test conditions attached to every figure: peel stress vs speed and interface, the green modulus envelope (0.2–0.8 GPa, median 0.4), green tensile vs degree of cure, a measured as-printed strut fracture stress. Cites the five PDFs beside it by filename. | You need a literature value *with* its test conditions. **Its "what this changes in our config" recommendations are pre-plate-5 and are not current guidance.** |
| [`literature/lit-green-peel.md`](literature/lit-green-peel.md) | The earlier confidence-classed review (MEASURED-PEER-REVIEW / MEASURED-COMMUNITY / RULE-OF-THUMB / MARKETING). Source of the 12–13 kPa FEP peel figure and of the "no source quantifies sealed-cavity blowout" gap. | You want to know how much to trust a peel number. |
| `literature/*.pdf` | The five source papers: separation force vs speed and cross-section (FEP/PP); mechanical-property modelling vs degree of cure; effects of curing on photosensitive resins (SLA); auto-support traction-force constrained optimization; towards scarless support removal in VPP. | You are checking a quoted figure against its paper. |

## Physics audits of the sizing model

Formula-by-formula audits of the v4 Python engine, all against
`resin-print-prep` master `f02b7af`. The **reasoning** in these is still good;
several **constant values** in them are pre-plate-5 and stale —
[`../rust-migration/03-physics-and-constants.md`](../rust-migration/03-physics-and-constants.md)
§5 lists exactly which.

| file | what it is | read it when |
|---|---|---|
| [`physics_load_sizing.md`](physics_load_sizing.md) | The most complete audit: every sizing law, dimensional checks, the three not-obviously-reconciled power laws, and the "only peel is modeled — no gravity, buoyancy, density or viscosity" finding. | You are porting or changing a sizing formula. **Its constants table is pre-plate-5.** |
| [`physics_contacts_tips.md`](physics_contacts_tips.md) | Contact and tip geometry reasoning behind the tip band and ball factors. [`SPEC-EMBED-POLICY.md`](../specs/SPEC-EMBED-POLICY.md) cites it as evidence. | You are touching contact placement or tip sizing. |
| [`physics_printability.md`](physics_printability.md) | Angle limits, the clearest statement of what "SF 8" is a safety factor *of*, and radius sensitivities (`√F` for tension-floored members, `F^¼` for deflection-floored). | You are arguing about a safety factor or an overhang angle. |
| [`physics_transient_peel.md`](physics_transient_peel.md) | The build-time transient model: restraint activation sweep, worst free-cantilever span, `N^0.125`. | You are reasoning about loads *during* the build rather than at steady state. |
| [`physics_coverage.md`](physics_coverage.md) | Rationale, calibration and measured resource evidence for the A3/A4 coverage model — sampling, patch areas, why the gate is area-weighted. Non-normative companion to [`SPEC-A3-A4-COVERAGE`](../specs/SPEC-A3-A4-COVERAGE.md). | You are changing coverage sampling or its gate. |

## Design and port history

| file | what it is | read it when |
|---|---|---|
| [`port_plan.md`](port_plan.md) | Performance profile plus the parallelization / port strategy for the support engine. `SPEC-RUST-KERNEL.md` cites it as its basis. | You are planning or reviewing port work. |
| [`port_plan_codex_review.md`](port_plan_codex_review.md) | The independent adversarial architecture review of that plan, from a different model family. | You want the counter-argument before committing to the plan. |

## Audits and censuses

| file | what it is | read it when |
|---|---|---|
| [`TREANT-V4-PRINT-AUDIT.md`](TREANT-V4-PRINT-AUDIT.md) | The synthesis report over the resin research, literature review and physics audits — a plain-language account of what the v4 engine does and does not model, written against a real print. | You want the whole picture in one document. |
| [`AUDIT-ADDENDUM-mesh-classification.md`](AUDIT-ADDENDUM-mesh-classification.md) | Completes the above: what the 195 unreachable contacts actually were. Every action-changing call independently re-verified by a second agent. | You are chasing unreachable or unsupported contacts. |
| [`penetration_census.md`](penetration_census.md) | Exact-geometry census of every illegal model crossing in the delivered support forest. `SPEC-V5-GEOM.md` names it as its authority for the v5 exact-geometry core. | You are working on geometry legality or the v5 core. |
| [`acceptance-run-2026-07-24-contactfix.md`](acceptance-run-2026-07-24-contactfix.md) | How the live Phase-8 acceptance fixture (`scripts/tests/fixtures/phase8_relocation_input.json`) was derived, and in what order. | The acceptance fixture needs regenerating or explaining. |

## Retired

| file | what it is | read it when |
|---|---|---|
| [`RETIRED-TOOLS.md`](RETIRED-TOOLS.md) | Tombstone for the retired P11 cross-examination tooling and why it was retired. Referenced by `scripts/tools/p11_crossexam.py`. | You are about to resurrect a retired tool. |
