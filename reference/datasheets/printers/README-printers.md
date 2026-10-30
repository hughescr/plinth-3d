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

The current vendor record is
[`saturn-4-ultra-16k.md`](saturn-4-ultra-16k.md). Inline prose should cite the
record rather than restating its printer values.

See [`../README.md`](../README.md) for what belongs in a datasheet and what
does not.
