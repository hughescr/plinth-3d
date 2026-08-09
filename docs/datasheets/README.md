# Datasheets — vendor and hardware reference data

**A datasheet is a document somebody else published about a thing we buy.**
Resin technical data sheets, printer physical parameters, material property
tables from a manufacturer. Nothing here is ours, and nothing here is a
finding: these are inputs to the physics model, not results from it.

The distinction matters because the engine's constants are typed by where they
came from. A value taken from a page in this directory is a vendor figure with
a citable source; a value the project measured for itself is not, and the two
are never allowed to be confused. See [`../physics.md`](../physics.md) for how
provenance is carried, and [`../development.md`](../development.md) for the
procedure that registers a new constant.

## Layout

- `resins/` — per-resin material data (green-state modulus and strength, cure
  behaviour, vendor-stated properties, water-washable quirks), plus `pdfs/`
  holding the manufacturers' own TDS files, and `RESIN-ENVELOPE.md`, the
  cross-manufacturer property envelope assembled from them.
- `printers/` — per-printer physical parameters. See
  [`printers/README.md`](printers/README.md).

## How to read a page here

Every value should carry the vendor source it came from, and should say
plainly when the vendor did **not** specify something. "Not specified" is a
useful entry: it stops a later reader from inventing a number and believing it
was published.

Vendor figures are also not measurements of *our* process. A published layer
range is a capability, not a calibration; a published separation-force
characteristic depends on the release film it was measured with. Where the
engine needs a number the vendor does not supply, that number is registered as
an unmeasured placeholder rather than quietly borrowed from a nearby vendor
figure.
