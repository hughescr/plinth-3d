// Home Support Calibration P1 - printable field protocol
// Source companion to home-support-calibration-p1-protocol.md

#set page(
  paper: "us-letter",
  margin: (top: 0.62in, bottom: 0.58in, left: 0.62in, right: 0.62in),
  header: align(right, text(size: 8pt, fill: rgb("#5b6673"))[HOME SUPPORT CALIBRATION P1 - FIELD PROTOCOL]),
  footer: context align(center, text(size: 8pt, fill: rgb("#5b6673"))[Controlled worksheet - page #counter(page).display("1")]),
)
#set text(font: ("TeX Gyre Adventor", "Helvetica Neue", "Arial", "DejaVu Sans"), size: 9.4pt)
#set par(leading: 0.52em, spacing: 0.52em)

#let navy = rgb("#183b5b")
#let teal = rgb("#147a85")
#let pale = rgb("#e8f2f3")
#let warm = rgb("#fff2cc")
#let grey = rgb("#5b6673")
#let check = box(width: 8pt, height: 8pt, stroke: 0.65pt + grey)
#let rule = line(length: 100%, stroke: 0.6pt + navy)
#let blank = line(length: 2.25in, stroke: 0.45pt + grey)
#let section(title) = [
  #v(0.45em)
  #text(size: 11pt, weight: "bold", fill: navy)[#title]
  #v(0.18em)
  #rule
  #v(0.22em)
]
#let note(body) = block(fill: pale, inset: 7pt, radius: 2pt)[#body]
#let warning(body) = block(fill: warm, inset: 7pt, radius: 2pt)[#body]
#let cell(body) = table.cell(inset: 4pt)[#body]
#let head(body) = table.cell(fill: navy, inset: 4pt)[#text(fill: white, weight: "bold")[#body]]

#show heading.where(level: 1): it => [
  #v(0.35em)
  #text(size: 17pt, weight: "bold", fill: navy)[#it.body]
  #v(0.2em)
]
#show heading.where(level: 2): it => section(it.body)

= Home Support Calibration P1
#text(size: 12pt, fill: teal, weight: "bold")[Cured-state home test protocol and printable data record]

#note[
  #text(weight: "bold")[Purpose.] This screening run records end-to-end print survival and limited cured-state handling. It does not measure wet-green modulus, peel force, universal resin strength, or a safe support policy by itself.
]

== Start here - non-negotiable boundary

#warning[
  #text(weight: "bold")[Do not touch, measure, load, remove, straighten, or rescue resin-wet parts.] Print, drain, wash, completely dry, and fully cure plate, stand, lever, and comb before separating anything. Use the resin maker's normal workflow and ordinary resin PPE, ventilation, containment, and waste practice. Record actual wash, dry, and cure settings and times.
]

#table(
  columns: (18pt, 1fr), stroke: 0.35pt + grey, inset: 4pt,
  table.header(head[ ], head[Before any handling]),
  check, cell[Keep each carrier strip intact through wash, dry, and cure.],
  check, cell[Inspect wash basket and vat/film for detached material before scoring. A fractured terminal or catcher engagement is a failure.],
  check, cell[Open plate and fixture projects in ChituBox without repair, rescale, auto-support, or auto-repair. Record screenshots.],
  check, cell[Photograph clean build plate and release film before print; photograph full cured plate from top, +X, and +Y before separation.],
)

== Artifact identity

#table(
  columns: (1.45in, 1fr), stroke: 0.35pt + grey,
  table.header(head[Artifact], head[Record]),
  cell[Actual printed plate], cell[`H2-exact-grounded-full-plate.chitubox` - reviewed field-patched artifact, not generator output.],
  cell[Plate SHA-256 / bytes], cell[`a33b2e10c1293b322f89fe2910c79055fa59b2d8b7bf3b2d4df64c804a53a679` / `26101476`],
  cell[Generator authority], cell[`/private/tmp/plinth-home-calibration-plate`, commit `1541379c1063ca9a41ca2761a189779a8f715a2e`; its JSON and CSV identify 76 specimens. Generator commits `634abdb` and `1541379` are not claimed to have produced H2.],
  cell[Fixture project], cell[`/private/tmp/plinth-home-loading-fixture-inspection/home-support-loading-fixture.chitubox`, commit `1541379c1063ca9a41ca2761a189779a8f715a2e`],
  cell[Fixture SHA-256 / bytes], cell[`bef224e1d9634971827e0e989f50b7bbdc288f08dd244e301ad0d7ef27ba1e1f` / `56516`],
)

#note[
  The owner opened and sliced the fixture cleanly. It is a real reviewed project, but the cured printed fixture remains uncalibrated until the individual stand, lever, and comb are inspected and measured.
]

== Cured inspection and scoring

For each of the 76 immutable IDs, score exactly one outcome: `present_intact`, `missing`, `cracked`, `permanently_bent`, `delaminated`, `brace_separated`, or `catcher_engaged`. A one-plate run is valid to screen and report. Three independently prepared plates are required only before promoting a tested cell into a measured engine constant.

#table(
  columns: (1.2in, 1fr), stroke: 0.35pt + grey,
  table.header(head[Strip], head[IDs and condition]),
  cell[S1-VERTICAL], cell[`V11 V12 V13 V14 V21 V22 V23 V24 V31 V32 V33 V34 V41 V42 V43 V44` - vertical, spans 6/12/18/24 mm x radii 0.30/0.50/0.70/0.90 mm.],
  cell[S2-INCLINED-Y], cell[`Y11 Y12 Y13 Y14 Y21 Y22 Y23 Y24 Y31 Y32 Y33 Y34 Y41 Y42 Y43 Y44` - 45 degrees toward project +Y; same matrix.],
  cell[S3A-INCLINED-X], cell[`X11 X13 X21 X23 X31 X33 X41 X43` - 45 degrees toward project +X; 0.30/0.70 mm radii.],
  cell[S3B-INCLINED-X], cell[`X12 X14 X22 X24 X32 X34 X42 X44` - 45 degrees toward project +X; 0.50/0.90 mm radii.],
  cell[S4-BRACED], cell[`BX50 BX51 BX52 BX90 BX91 BX92 BY50 BY51 BY52 BY90 BY91 BY92` - 18 mm detachable; +X/+Y, radius 0.50/0.90 mm, brace none/single/X.],
  cell[S5-TILES-CONTROLS], cell[`T01 T02 T03 T04 T05 T06 T07 T08 CS1 CM1 CS2 CM2 CS3 CM3 CS4 CM4` - removal tiles; four strong CS and four marginal CM controls.],
)

== Fixture validation and safe loading

#warning[
  Wear eye protection. Place a clear shield or clear bag around the coupon and hanging mass. Never put hands or a caliper near a loaded brittle coupon or below the hanging mass.
]

1. After full cure, separate stand, lever, and 0.5/1.0/2.0 mm comb from their carriers. Place lever in stand.
2. Cut only a detachable coupon from its bridge. Seat its base in receiver `18` at x=58 mm for an 18 mm coupon, or receiver `24` at x=64 mm for a 24 mm coupon. Shaft points toward pivot; terminal crossbar sits at x=40 mm under the lever paddle.
3. Confirm the free lever moves without preload, binding, or unintended contact. Use a closed loop around the straight rooted mass peg. At x=-10 mm the hanging assembly pulls down and paddle applies the intended vertical load.
4. Caliper-measure pivot-to-mass arm `a_m` and pivot-to-contact arm `a_s` three times, unloaded. Nominal geometry is 10.0 mm / 40.0 mm, a 4:1 force reduction. Use actual ratio if different.
5. First validate with `CM1`, then `CS1`, recording both outcomes. Then test remaining controls and braced coupons in recorded geometry-matched pairs. Never reuse a damaged coupon.

#table(
  columns: (1.1in, 0.95in, 0.95in, 1fr), stroke: 0.35pt + grey,
  table.header(head[Assembly mass], head[Effective load], head[Nominal force], head[Record]),
  cell[20 g], cell[5 g], cell[0.0490 N], cell[exact complete assembly mass: object/container + loop/string],
  cell[40 g], cell[10 g], cell[0.0981 N], cell[weigh on kitchen scale; no coins],
  cell[80 g], cell[20 g], cell[0.1961 N], cell[record calculated force using measured arms],
  cell[160 g], cell[40 g], cell[0.3923 N], cell[10 s dwell, 30 s unloaded recovery, then ten cycles],
  cell[320 g], cell[80 g], cell[0.7845 N], cell[advance only if prior level survives],
)

#set text(size: 8.8pt)
#text(weight: "bold")[Force:] `F_specimen_N = m_g / 1000 x 9.80665 x (a_m / a_s)`  #h(1em) #text(weight: "bold")[Moment:] `M_base_Nmm = F_specimen_N x measured_base_to_contact_span_mm`

During load, use fixed perpendicular phone photos with the printed comb in the specimen plane at zero and at every mass. Only use the caliper unloaded: baseline and residual/recovery after 30 s. Treat changes under 0.10 mm as `unresolved`.

#warning[
  Stop at the first audible crack, visible whitening, brace separation, lever unseating, snap, or permanent set greater than 1.0 mm. Record the first stop. Later levels are `not_attempted`, never passes.
]

#pagebreak()

= Printable record sheets
#text(size: 10pt, fill: teal)[Print or copy these forms unchanged. A blank is not a pass.]

== A. Run metadata

#table(
  columns: (2.25in, 1fr), stroke: 0.35pt + grey, inset: 5pt,
  table.header(head[Field], head[Record]),
  cell[Run ID / date / operator], cell[#blank],
  cell[Printer / slicer version / profile digest], cell[#blank],
  cell[Resin product / lot], cell[#blank],
  cell[Print date / temperature window / actual], cell[#blank],
  cell[PFA type / install-use state / inspection], cell[#blank],
  cell[Wash equipment/settings/time], cell[#blank],
  cell[Dry method/time and confirmation], cell[#blank],
  cell[Cure equipment/settings/time and confirmation], cell[#blank],
  cell[Basket inspection / vat-film inspection], cell[#blank],
  cell[Plate visual gate and photo file], cell[#blank],
  cell[Screening or descriptive-only reason], cell[#blank],
)

== B. 76-ID survival matrix

#set text(size: 8.1pt)
#table(
  columns: (0.47in, 0.77in, 0.47in, 0.77in, 0.47in, 0.77in, 0.47in, 0.77in), stroke: 0.3pt + grey, inset: 3pt,
  table.header(head[ID], head[Outcome/photo], head[ID], head[Outcome/photo], head[ID], head[Outcome/photo], head[ID], head[Outcome/photo]),
  cell[V11], cell[ ], cell[V12], cell[ ], cell[V13], cell[ ], cell[V14], cell[ ],
  cell[V21], cell[ ], cell[V22], cell[ ], cell[V23], cell[ ], cell[V24], cell[ ],
  cell[V31], cell[ ], cell[V32], cell[ ], cell[V33], cell[ ], cell[V34], cell[ ],
  cell[V41], cell[ ], cell[V42], cell[ ], cell[V43], cell[ ], cell[V44], cell[ ],
  cell[Y11], cell[ ], cell[Y12], cell[ ], cell[Y13], cell[ ], cell[Y14], cell[ ],
  cell[Y21], cell[ ], cell[Y22], cell[ ], cell[Y23], cell[ ], cell[Y24], cell[ ],
  cell[Y31], cell[ ], cell[Y32], cell[ ], cell[Y33], cell[ ], cell[Y34], cell[ ],
  cell[Y41], cell[ ], cell[Y42], cell[ ], cell[Y43], cell[ ], cell[Y44], cell[ ],
  cell[X11], cell[ ], cell[X13], cell[ ], cell[X21], cell[ ], cell[X23], cell[ ],
  cell[X31], cell[ ], cell[X33], cell[ ], cell[X41], cell[ ], cell[X43], cell[ ],
  cell[X12], cell[ ], cell[X14], cell[ ], cell[X22], cell[ ], cell[X24], cell[ ],
  cell[X32], cell[ ], cell[X34], cell[ ], cell[X42], cell[ ], cell[X44], cell[ ],
  cell[BX50], cell[ ], cell[BX51], cell[ ], cell[BX52], cell[ ], cell[BX90], cell[ ],
  cell[BX91], cell[ ], cell[BX92], cell[ ], cell[BY50], cell[ ], cell[BY51], cell[ ],
  cell[BY52], cell[ ], cell[BY90], cell[ ], cell[BY91], cell[ ], cell[BY92], cell[ ],
  cell[T01], cell[ ], cell[T02], cell[ ], cell[T03], cell[ ], cell[T04], cell[ ],
  cell[T05], cell[ ], cell[T06], cell[ ], cell[T07], cell[ ], cell[T08], cell[ ],
  cell[CS1], cell[ ], cell[CM1], cell[ ], cell[CS2], cell[ ], cell[CM2], cell[ ],
  cell[CS3], cell[ ], cell[CM3], cell[ ], cell[CS4], cell[ ], cell[CM4], cell[ ],
)

#pagebreak()
#set text(size: 9.2pt)

== C. Fixture inspection and arm measurements

#table(
  columns: (2.6in, 1fr), stroke: 0.35pt + grey, inset: 5pt,
  table.header(head[Field], head[Record]),
  cell[Fixture opened/sliced clean; screenshot/file], cell[#blank],
  cell[Stand, lever, comb washed/dried/cured before separation], cell[#blank],
  cell[Eye protection, clear shield/bag, no hands under mass], cell[#blank],
  cell[Receiver, terminal x=40, shaft toward pivot, paddle contact], cell[#blank],
  cell[Mass loop seated at rooted peg base], cell[#blank],
  cell[Free lever movement/no preload], cell[#blank],
  cell[Intended vertical load direction], cell[#blank],
  cell[`a_m` readings 1 / 2 / 3 mm; selected], cell[#blank],
  cell[`a_s` readings 1 / 2 / 3 mm; selected], cell[#blank],
  cell[Actual `a_m / a_s` ratio; nominal 10.0/40.0 verified?], cell[#blank],
  cell[Measured base-to-contact specimen span mm], cell[#blank],
  cell[Fixture disposition: ready / fixture_unseated / fixture_invalid], cell[#blank],
)

== D. Per-coupon load sheet

Coupon ID: #blank #h(1em) Strip: #blank #h(1em) Receiver 18 or 24: #blank

Unloaded baseline readings (mm): #blank / #blank / #blank  Selected: #blank

Zero-load perpendicular photo with comb: #blank

#set text(size: 8.05pt)
#table(
  columns: (0.48in, 0.72in, 0.62in, 0.72in, 0.72in, 0.88in, 0.78in, 0.6in, 0.9in), stroke: 0.3pt + grey, inset: 3pt,
  table.header(head[Target g], head[Assembly g], head[Force N], head[10 s photo], head[Peak photo defl.], head[30 s readings], head[Recovery], head[10 cycles], head[Outcome/stop]),
  cell[20], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ],
  cell[40], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ],
  cell[80], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ],
  cell[160], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ],
  cell[320], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ],
)

First stop level: #blank  Later levels: `not_attempted` if stopped.

#pagebreak()

== E. T01-T08 removal and scar sheet

Tool order and removal direction fixed before first tile: #blank

#set text(size: 8.2pt)
#table(
  columns: (0.42in, 0.76in, 0.6in, 0.74in, 0.72in, 0.78in, 0.7in, 0.56in, 0.92in), stroke: 0.3pt + grey, inset: 3pt,
  table.header(head[ID], head[Start/end], head[Time], head[Tile frac.?], head[Support frac.?], head[Remnant mm], head[Scar mm], head[Touch-up], head[Before/after photo]),
  cell[T01], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ],
  cell[T02], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ],
  cell[T03], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ],
  cell[T04], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ],
  cell[T05], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ],
  cell[T06], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ],
  cell[T07], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ],
  cell[T08], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ], cell[ ],
)

Report a scar/remnant below threshold as `<0.10 mm`.

== F. Photo, file, deviation, and report-to-Codex checklist

#table(
  columns: (18pt, 1fr, 1.55in), stroke: 0.35pt + grey, inset: 4pt,
  table.header(head[ ], head[Item], head[File/path or note]),
  check, cell[Build plate and release-film pre-print photo], cell[ ],
  check, cell[Plate and fixture ChituBox inspections], cell[ ],
  check, cell[Full cured plate top, +X, +Y photographs], cell[ ],
  check, cell[Basket and vat/film detached-material photos], cell[ ],
  check, cell[Fixture seating and arm-measurement photos], cell[ ],
  check, cell[Zero and each loaded perpendicular-comb photo], cell[ ],
  check, cell[T01-T08 before/after photos], cell[ ],
  check, cell[Raw data, deviations, exclusions, and control disposition], cell[ ],
  check, cell[Screening conclusion and limitations; no one-plate constant promotion], cell[ ],
  check, cell[Final report package path sent to Codex], cell[ ],
)
