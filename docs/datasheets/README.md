# Datasheets — external reference data

Three kinds of document live under `docs/`, and the distinction is what
decides where a new file goes:

- **`docs/specs/`** — normative rules we ratified. Code cites these as law.
- **`docs/datasheets/`** — external reference data about materials and
  hardware that *parameterises* the physics model. Not ours, not a run
  output: vendor datasheets, measured literature values, printer physical
  parameters. Inputs to the model.
- **artifacts** — evidence produced by running a particular version of the
  code (results JSON, profiles, censuses, screenshots). These are outputs,
  not revisions to the code or the docs, so they are **not kept in git** —
  they are archived outside the repo.

## Layout

- `resins/` — per-resin material data (green-state modulus and strength,
  cure behaviour, vendor-stated properties, water-washable quirks).
- `printers/` — per-printer physical parameters. See its README.
- `lit_green_peel.md` — the confidence-classed literature review behind the
  peel-force and green-state numbers. It spans resin *and* process, so it
  sits at this level rather than under either subdirectory.

## Why this directory matters

`scripts/support_placement/config.py` currently hardcodes physical constants
whose own comments admit they are unmeasured — e.g. `peel_pressure_pa`
("NOT measured for this printer/resin yet ... 3.0 kPa is the geometric mean
of a plausible 1-5 kPa range") and `shaft_drag_force_n` ("a ROUGH ESTIMATE
(order-of-magnitude guess ...)"). Those are exactly the values a datasheet
should eventually supply, per resin and per printer, instead of a single
baked-in guess.

Nothing is wired up that way today. This is a note about intent, not a
description of current behaviour.
