# Printer datasheets

One file per printer, holding its **physical parameters** — the values that
are inputs to the physics model, never outputs of a run:

- plate size (X × Y build area, Z height)
- XY resolution / pixel pitch (e.g. 14 × 19 µm)
- layer-height range (min / default / max)
- lift and retract speeds, and lift distance
- peel / separation force characteristics, and the release-film type they
  were measured with (FEP, nFEP, ACF — the film changes the number)
- anything else per-printer that the model reads rather than derives

Nothing lives here yet. Today the only printer facts in the repo are inline
prose: `README.md` and `skills/prep-mini/references/sizing.md` both name the
Elegoo Saturn 4 Ultra 16K at 14 × 19 µm. When a real datasheet is written,
it goes here as `saturn-4-ultra-16k.md` and those prose mentions should cite
it instead of restating the numbers.

See `../README.md` for how datasheets differ from specs and from run
artifacts.
