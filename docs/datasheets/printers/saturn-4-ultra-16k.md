# ELEGOO Saturn 4 Ultra 16K — vendor specification record

**Model identity.** Canonical model: **ELEGOO Saturn 4 Ultra 16K**. The official
product page also renders the name as “Saturn 4 Ultra 16k” in its box contents;
that is a capitalization variant only. **Do not treat “Saturn 4 Ultra” (without
“16K”) as an alias:** Elegoo presents it as a separate comparison model with a
different LCD resolution and XY resolution.

**Scope and provenance.** This is a transcription of vendor-published hardware
specifications, not a calibration plate, a measured printer characterization, or
measured peel-force evidence. In particular, a PFA film listing and a tilt-release
claim do not supply a separation-force value for this printer.

## Sources and verification

| ID | Official Elegoo source | Publication / version | Retrieved | What was cross-checked |
|---|---|---|---|---|
| P | [Saturn 4 Ultra 16K — Best 3D Printer for Beginners & Pros](https://www.elegoo.com/products/saturn-4-ultra-16k-10inch-monochrome-lcd-resin-3d-printer) | No publication date or document version shown | 2026-07-31 | Product technical specifications: build volume, LCD pixel resolution, vendor XY resolution, layer-thickness range, maximum speed, tilt release, PFA film, and machine dimensions. |
| R | [Elegoo Unveils Saturn 4 Ultra 16K: A Major Leap in Accuracy and User Experience](https://www.elegoo.com/en-gb/blogs/news/elegoo-unveils-saturn-4-ultra-16k-a-major-leap-in-accuracy-and-user-experience) | Press release, 2025-01-22 | 2026-07-31 | Independently confirms 14 × 19 μm XY resolution, 0.02 mm vendor Z-axis accuracy, 211.68 × 118.37 × 220 mm build volume, 150 mm/h maximum speed, and that the resin tank tilts for release. |

The values shared by P and R were independently re-read on 2026-07-31 before
this record was committed. P is the source for the fields R does not publish.

## Physical parameters

| Field | Value | Status / interpretation | Source |
|---|---:|---|---|
| Build volume (vendor order) | **211.68 × 118.37 × 220 mm** | Vendor-published ordered triplet. Neither source labels its components X, Y, and Z. | P, R |
| LCD | 10-inch 16K monochrome LCD | Vendor description. | P, R |
| LCD pixel resolution (vendor order) | **15,120 × 6,230 pixels** | Vendor-published ordered pair; neither source assigns a component to X or Y. | P |
| XY pixel pitch (vendor order) | **14 × 19 μm** | Vendor-published ordered pair. The pitches are anisotropic, but the vendor sources do not assign either component to a named axis. | P, R |
| Z-axis accuracy | 0.02 mm | Vendor-described Z-axis accuracy; it is not a layer-height default. | P, R |
| Vendor layer-thickness capability | **0.01–0.20 mm** | Vendor range. | P |
| Project default layer height | **0.050 mm** | Project printer-profile setting, **not** a claimed Elegoo default and not a substitute for resin/process calibration. It is carried as a registered project constant with its own provenance, not as a vendor figure. | Local project record |
| Vendor default layer height | **Not specified** | P gives a range, not a default. | P |
| Printer dimensions | **327.4 × 329.2 × 548 mm** | The vendor lists this ordered triplet as “Printer Dimensions”; axis labels / orientation for the three enclosure dimensions are not specified. | P |
| Release mechanism | **Tilt Release** | R states that the resin tank tilts to peel the model from the release film. | P, R |
| Tilt direction / physical-axis mapping | **Not specified** | Neither source maps the tilt to printer X/Y, front/back, or a sweep direction. | P, R |
| Release film | **PFA** | Vendor feature/specification. Film thickness, surface finish, and tension are not specified. | P |
| Maximum printing speed | **150 mm/h** | Vendor maximum; it is not an independently measured speed or a per-motion lift/retract setting. | P, R |
| Lift speed | **Not specified** | No numeric lift-motion speed in the official sources above. | P, R |
| Retract speed | **Not specified** | No numeric retract-motion speed in the official sources above. | P, R |
| Lift distance | **Not specified** | No numeric lift distance in the official sources above. | P, R |
| Retract distance | **Not specified** | No numeric retract distance in the official sources above. | P, R |
| Peel / separation force or pressure | **Not specified** | No vendor force, pressure, displacement, resin, film-condition, or test-method evidence was found in these sources. | P, R |

## Derived project axis mapping (not a vendor axis label)

For project use, map the first vendor build-volume, pixel-resolution, and
pixel-pitch components to **X**, the second components to **Y**, and the third
build-volume component to **Z**. This is a project mapping, not a statement that
Elegoo labels those axes. The exact first-to-first and second-to-second arithmetic
demonstrates why the X/Y components must not be swapped:

| Project axis | Corresponding vendor-order components | Derivation |
|---|---|---|
| X | first: 211.68 mm; 15,120 px; 14 μm | 211.68 mm / 15,120 px = 0.014 mm/px = **14 μm/px** |
| Y | second: 118.37 mm; 6,230 px; 19 μm | 118.37 mm / 6,230 px = 0.019 mm/px = **19 μm/px** |
| Z | third: 220 mm | Project designation for the third vendor build-volume component; no vendor X/Y/Z label was found. |

## Use boundary

Use the X/Y pixel pitch and vendor-stated machine geometry as hardware inputs only.
Do not infer peel force, film tension, release distance, or a resin-specific print
profile from this record. Any such value needs a condition-bearing measurement or
a separately cited vendor source.

**Project-profile disposition.** The Rust profile may record the vendor-order
build volume and pitch as `X/Y/Z = 211.68/118.37/220 mm` and
`pitch X/Y = 14/19 µm`, because the project mapping above is explicit and the
vendor evidence is known. `leading_release_edge = NegativeY`,
`release_front_sweep_direction = PositiveY`, the 0.050 mm layer default, and the
5.0 mm lift default are local project records, not vendor claims. In particular,
the 5.0 mm lift is a diagnostic `Placeholder`; this datasheet supplies no lift
distance.
