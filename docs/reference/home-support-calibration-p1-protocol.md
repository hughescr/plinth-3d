# Home Support Calibration P1 — home-test protocol

## Status and authority

This is a home-printer process-screening protocol for the generated
`HOME-SUPPORT-CAL-P1` plate. It measures whether listed geometries survive the
recorded production workflow and limited, cured-state tabletop handling. It is
not a material-characterisation test: it does not establish wet-green modulus,
printer peel force, universal resin strength, or a safe support policy by itself.

The geometry authority is the generated package at
`/private/tmp/plinth-home-calibration-plate`, commit
`1541379c1063ca9a41ca2761a189779a8f715a2e`, specifically its generated
`home-support-calibration-manifest.json` and CSV. The package identifies a
nominal fixture, `HOME-CAL-LEVER-4TO1`, but **does not contain a fixture
artifact**. Therefore the loading phase below is prohibited until a separately
reviewed fixture is available and its setup record is complete. This protocol
does not treat the nominal dimensions as proof that a fixture exists or is safe.

The plate is six self-rafted, contiguous carrier strips (76 specimens total),
within a 200.5 × 115.25 × 26.75 mm envelope. X and Y mean project axes only;
they are not asserted to be physical printer tilt axes.

## Non-negotiable safety and handling boundary

Do not touch, measure, load, remove, straighten, or rescue a resin-wet
specimen. Print, drain, wash, completely dry, and fully cure the whole plate
first, using the resin manufacturer's normal workflow and the same PPE,
ventilation, containment, and waste handling used for ordinary resin prints.
Record the actual wash, dry, and cure equipment/settings/times; this protocol
does not invent substitute settings.

Keep each strip intact through wash, dry, and cure. The 12 mm (S1–S3) or 16 mm
(S4–S5) carrier width is the handling feature; coupons are separated only after
full cure, only when the manifest marks them `detachable_for_loading=true`.
The open cradles and catchers are non-load-bearing containment aids, not
specimen supports. A fractured terminal or a catcher engagement is a failed
specimen. Inspect both wash basket and vat/film for detached material before
scoring and record what was found.

## Preregistration and print gate

Before slicing, create one run record and pin all of the following:

| Field | Required record |
|---|---|
| Geometry | Source commit above; SHA-256 and byte size of the accepted `.chitubox`, JSON, and CSV |
| Printer/profile | Saturn 4 Ultra 16K; slicer version; unchanged production profile export or every displayed setting |
| Resin/process | resin product and lot; print date; tank-temperature window and actual temperature; PFA/release-film type, installation/use state, and inspection result |
| Visual gate | ChituBox opens without repair, rescale, missing-model, out-of-bounds, auto-support, or auto-repair action; six carrier strips and labels are legible |
| Repeats | three independently prepared plates, not repeated loading cycles on one plate |

Photograph the clean build plate and release film before the run. A visibly
damaged film, non-production resin, or changed profile makes the run descriptive
only; retain all observations rather than silently retrying it.

## Exact inventory and eligibility

The ID printed on the strip and the manifest position are joint identity
channels. Record every ID below exactly once in the survival table. The canonical
CSV supplies each specimen's exact base and terminal coordinates and dimensions;
do not reconstruct them from this summary.

| Strip | Count | Immutable IDs | Condition |
|---|---:|---|---|
| `S1-VERTICAL` | 16 | `V11 V12 V13 V14 V21 V22 V23 V24 V31 V32 V33 V34 V41 V42 V43 V44` | vertical; spans 6/12/18/24 mm × radii 0.30/0.50/0.70/0.90 mm |
| `S2-INCLINED-Y` | 16 | `Y11 Y12 Y13 Y14 Y21 Y22 Y23 Y24 Y31 Y32 Y33 Y34 Y41 Y42 Y43 Y44` | 45° toward project +Y; same span/radius matrix |
| `S3A-INCLINED-X` | 8 | `X11 X13 X21 X23 X31 X33 X41 X43` | 45° toward project +X; radii 0.30/0.70 mm |
| `S3B-INCLINED-X` | 8 | `X12 X14 X22 X24 X32 X34 X42 X44` | 45° toward project +X; radii 0.50/0.90 mm |
| `S4-BRACED` | 12 | `BX50 BX51 BX52 BX90 BX91 BX92 BY50 BY51 BY52 BY90 BY91 BY92` | 18 mm, detachable; +X/+Y × radius 0.50/0.90 mm × brace none/single/X |
| `S5-TILES-CONTROLS` | 16 | `T01 T02 T03 T04 T05 T06 T07 T08 CS1 CM1 CS2 CM2 CS3 CM3 CS4 CM4` | removal tiles; four `CS` must-survive controls and four deliberately marginal `CM` controls; only CS/CM are detachable |

Only the 20 detachable specimens (`BX*`, `BY*`, `CS*`, `CM*`) may be cut free
after cure for the fixture phase. Do not substitute a non-detachable specimen.

## Cured-state inspection, photographs, and caliper readings

After full cure and before any separation, photograph the complete plate from
top, +X side, and +Y side, with the run ID visible. For each immutable ID, score
one outcome: `present_intact`, `missing`, `cracked`, `permanently_bent`,
`delaminated`, `brace_separated`, or `catcher_engaged`.

Use the 0–150 mm digital caliper advertised ±0.02 mm only after full cure. For
each intact, detachable coupon selected for loading, take three independent
readings (remove and reseat between readings) at these fixed points:

1. pivot-to-mass point and pivot-to-specimen-contact point on the verified
   fixture (both in mm);
2. base datum to terminal-centre baseline displacement in the intended load
   direction, before loading;
3. the same datum-to-terminal-centre displacement immediately after each dwell
   and after recovery.

Keep all three readings, report their range, and retain the photographs showing
the datum and load direction. Treat a change smaller than 0.10 mm as
`unresolved`; the advertised ±0.02 mm is not a claim that hand placement on a
resin part resolves 0.02 mm. The 0.5/1.0/2.0 mm visual gates are only a quick
check, not a precision result.

For `T01`–`T08`, use the same tool order and removal direction for every tile.
Record removal time, tile fracture, support fracture, largest remnant height,
largest scar width, and touch-up time. Report a scar/remnant below 0.10 mm as
`<0.10 mm`, not an invented exact value.

## Verified 4:1 fixture gate and load schedule

Loading begins only when the reviewed fixture is identified as
`HOME-CAL-LEVER-4TO1` and the following setup record is photographed and
completed for that individual fixture:

1. The selected cured coupon is seated without forcing it, with its shaft
   horizontal, terminal at the documented contact location, and load applied in
   the recorded lateral direction. The fixture must remain supported and stable
   throughout the test.
2. Measure and record the pivot-to-mass arm `a_m` and pivot-to-contact arm
   `a_s` with the caliper. The nominal geometry is `a_m = 10.0 mm` and
   `a_s = 40.0 mm`, i.e. a 1:4 arm ratio / 4:1 force reduction. If the measured
   setup is not 10.0/40.0, record the actual ratio and calculate with it; never
   label it a verified nominal 4:1 setup.
3. Photograph the unloaded seating and the measured arms. If it rocks,
   unseats, contacts an unintended surface, or the load path is unclear, stop:
   record `fixture_unseated`/`fixture_invalid` and do not load the coupon.

Use a kitchen scale and a dense object or closed screw-top container. Record the
exact displayed mass in grams for every application; do not use coins or assume
the marked mass of any object. The conservative preregistered ascending schedule
is:

| Exact target input mass | Nominal direct-load equivalent at 10:40 | Nominal force at specimen |
|---:|---:|---:|
| 20 g | 5 g | 0.0490 N |
| 40 g | 10 g | 0.0981 N |
| 80 g | 20 g | 0.1961 N |
| 160 g | 40 g | 0.3923 N |
| 320 g | 80 g | 0.7845 N |

For the actual weighed mass `m_g`, calculate rather than round:

```text
F_specimen_N = m_g / 1000 × 9.80665 × (a_m / a_s)
M_base_Nmm  = F_specimen_N × measured_load_height_mm
```

Apply each surviving level for a 10 s dwell. Unload for 30 s, record the
immediate and recovered datum-to-terminal displacement, then complete ten
load/unload cycles at that level before advancing. Record: mass, calculated
force, dwell result, peak observed deflection, residual deflection after 30 s,
cycle count, and outcome. The cycles characterize one cured coupon only; they
are not print replicates.

Stop immediately at the first audible crack, visible whitening, brace
separation, lever unseating, snap, or permanent set greater than 1.0 mm. Do not
continue a damaged coupon to obtain a higher failure number. Record the first
stop level and failure mode; later levels are `not_attempted`, not passes.

## Validity, results package, and limitations

Report every plate, including failed controls and descriptive-only runs. A run
cannot support a safe policy unless all four `CS` controls survive and at least
one `CM` control demonstrates intended marginal sensitivity. A failed print is
evidence, not an optional retry. Three independently prepared plates are
required before promoting any exact cell from this protocol.

The dated report package must contain:

- this preregistration and source commit/digests;
- printer, resin, film, temperature, profile, wash/dry/cure, and fixture setup
  records;
- raw per-ID survival, caliper, removal, mass/force, dwell, recovery, cycle,
  and failure tables;
- before/after/full-plate/fixture photographs, including all detached-material
  inspections;
- deviations, exclusions, control disposition, and all descriptive-only runs;
- an explicit statement that conclusions are limited to the recorded printer,
  resin lot, profile, film state, temperature window, workflow, and fixture.

Do not claim that this screen separates wet-resin stiffness from peel load, maps
project axes to a physical tilt axis, establishes a universal failure force, or
converts a one-off survival into a `Measured` physics value. Results may remain
inconclusive.
