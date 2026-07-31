# Datasheets — vendor and hardware reference data

**A datasheet is a document somebody else published about a thing we buy.**
Resin technical data sheets, printer physical parameters, material property
tables from a manufacturer. Nothing here is ours; nothing here is a finding.

Three kinds of document live under `docs/`, and the distinction is what
decides where a new file goes:

- **`docs/specs/`** — normative rules we ratified. Code cites these as law.
- **`docs/datasheets/`** — **vendor / hardware reference data only.** Data
  published by a manufacturer about a resin, a printer, or a piece of hardware
  we use, which *parameterises* the physics model. Inputs to the model, not
  ours.
- **`docs/reference/`** — **our own durable written findings**: measured
  calibration-plate results, physics audits, literature reviews, port and
  design history. Anything we worked out, wrote down, and still believe.
  See [`../reference/README.md`](../reference/README.md), which indexes every
  file there.
- **artifacts** — evidence produced by running a particular version of the
  code (results JSON, profiles, censuses, screenshots). These are outputs,
  not revisions to the code or the docs, so they are **not kept in git** —
  they are archived outside the repo.

> **Moved 2026-07-31.** The peel-force and green-state **literature** used to
> live here as `datasheets/physics/`. It is a review *we* wrote over papers
> *we* read, not a vendor document, so it now lives at
> [`../reference/literature/`](../reference/literature/) — same basenames,
> `LITERATURE-NOTES.md`, `lit-green-peel.md` and the five source PDFs, all
> together in one directory. Update any link you find pointing at the old path.

## Layout

- `resins/` — per-resin material data (green-state modulus and strength,
  cure behaviour, vendor-stated properties, water-washable quirks), plus
  `pdfs/` holding the manufacturers' own TDS files, and `RESIN-ENVELOPE.md`,
  the cross-manufacturer property envelope assembled from them.
- `printers/` — per-printer physical parameters. See its README.

## Why this directory matters

`scripts/support_placement/config.py` hardcodes physical constants whose own
comments admit they are unmeasured — `peel_pressure_pa` and
`shaft_drag_force_n` both describe themselves in-code as estimates rather than
measurements. Those are exactly the values a datasheet should eventually
supply, per resin and per printer, instead of a single baked-in guess.

Nothing is wired up that way today. This is a note about intent, not a
description of current behaviour. The Rust port's plan for making that
provenance explicit — `Measured` / `Derived` / `Placeholder` constants — is in
[`../rust-migration/03-physics-and-constants.md`](../rust-migration/03-physics-and-constants.md).
