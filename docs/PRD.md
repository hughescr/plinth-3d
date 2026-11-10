# Plinth PRD

Oct 29, 2026 · @Craig Hughes

Plinth turns a folder of miniature STLs into ChituBox files with editable, physics-sized supports, so more prints succeed, weak spots are flagged in advance, and the treant becomes practical. Sections 1–9 are the acceptance contract; section 10 lists implementation constraints. Sources, in order of authority: the owner interview, then the lessons from the first attempt, then reference facts.

How each value is labelled:

- **Owner ruling (date):** Craig decided it. Craig is not infallible: when a ruling appears to need review, the orchestrator raises it with him, with its reasons (section 11).
- **\[Orchestrator starting value\]:** the orchestrator chose it, because the owner delegated the choice or because it is a technical choice. It is in force now. It is tuned from print results, and the owner may change it at any time. It is never a measured fact.
- **\[Proposal\]:** a suggestion, not yet adopted.

## 0. Glossary

| Term | Meaning in this document |
| --- | --- |
| Model | One input mesh (one STL file). One kit part is one model. |
| Pose | The orientation of a model on the build plate. |
| Keep-up marker | A per-model mark: "this model is pre-oriented: keep its up direction up". Plinth may still turn it about the vertical axis to place it on the plate (owner ruling 2026-10-30). |
| Support | One printed structure that holds the model: contact, tip, column, base, and the raft that joins bases. Built from ChituBox's own support parts. |
| Raft | A thin layer on the build plate that joins the bases of supports to each other, for stability and to help remove the print from the plate (owner ruling 2026-10-30). His profile: 0.2 mm thick (P17). Plinth emits only the "connected" style, where each support base is linked to nearby bases; never the hex mat or ChituBox's plain default mat (owner ruling 2026-10-30). |
| Contact | The place where a support is meant to touch the model. Its size is its contact area in mm², never a diameter (owner ruling 2026-10-30). |
| Contact patch | The area of model surface that a support touches or enters, as measured in the emitted file (6.2). |
| Contact-surrounding region | The end of a support next to its own contact, inside which it may approach the model (6.2). |
| Incidental contact | A support touching the model outside any contact-surrounding region. |
| Fused contacts | Two or more contact patches that touch or merge. |
| Fragile part | A thin, long feature that breaks under small force: a sword blade, a finger, a thin wing edge (test in FR-24). |
| Peel load | The force that pulls each new layer off the vat film as the printer tilts. The main load supports carry. |
| Area ratio | An index for sizing a support, not a measured force: the peeling layer area the load model gives to one support section ÷ that section's cross-section area in the emitted file. The contact area on the model is a different number and never replaces the section area. |
| Unprintable ratio | The additive area ratio above which the load model calls a layout unprintable as is: 40, the owner's limit (P4). |
| Design ratio | The largest area ratio plinth allows at any support section (P5). |
| Island | A region of a layer not connected to anything printed below it. |
| Hollowing | Removing the inside of a large model, leaving a wall, to save resin and reduce peel load. |
| Cavity, drain, vent | A hollow space; a hole where resin runs out; a hole where air gets in. |
| Drain check pose | Drains need to work only in the print pose. He swirls parts in a wash station in no set orientation, so washing adds no pose to check (owner ruling 2026-10-30). |
| Approved repair | A mesh change the owner has approved in advance (FR-7). |
| Hard limit | A rule the final emitted geometry must never break (section 6). |
| Target | A goal plinth works toward, traded in the owner's priority order. |
| Reported quantity | A number plinth states but does not limit. |
| Warning | A located message in the report: what, where, why, how big, and any fix. |
| Intervention warning | A warning that asks the owner to act (an easy fix) or to accept a risk before printing. |
| Easy fix | An edit the owner calls easy: adding supports in ChituBox where its overhang detection flags a spot. Moving supports is a hard edit. |
| Generated file / edited file | The .chitubox plinth wrote / the same file after the owner's edits. Both are kept. |
| Diagnostic file | A file for a model plinth could not make printable. Never a print candidate (FR-31). |
| Owner baseline | His ChituBox practice and exported support profiles (test-data/chitubox-native/chitubox-support-specs.cfgx). |
| Independent checker | A program that shares no code with plinth's generator and measures the emitted file (NFR-6). |
| Orchestrator | The Claude session that runs this project: it plans, directs other AI agents, checks their work and reports to the owner. |
| AI model family | AI models from one vendor (Claude, GPT, DeepSeek). Unrelated to the miniature models in this document. |
| Test print | A small calibration print (old name: coupon) made only to measure a physical parameter. |
| Credible to print | The bar an output must pass before the owner spends resin on it (8.3). |

## 1. Problem and goal

### 1.1 The problem today

Craig prints D&D miniatures on an Elegoo Saturn 4 Ultra 16K MSLA resin printer. His method in ChituBox is a macro: auto-orient; "magic support" with the "light" preset (medium and smaller minis) or "better" (large and huge); overhang detection; hand-added ultralight supports on the few spots that look risky; slice; check the islands the slicer reports; print.

The problems, in his terms:

1. **Success is 75–90%.** Most failures are supports too flimsy to hold the model. He adjusts and reprints.
2. **He cannot tell ahead of time** whether a print will succeed.
3. **The preset choice is a guess.** "Light" versus "better" exists only because ChituBox has no physics model. It cannot tell when peel force will rip a model off its supports, so he guesses from model size.
4. **Large, complex models are impractical.** The Awakened Tree (the treant) takes ChituBox just over 5 minutes to auto-support on one core, plus a couple of minutes to hollow, also on one core. The UI freezes and Cancel acts only when the job ends. Supporting the treant well by hand is extremely difficult. The treant is a large part of why this project exists: he wants to print it for his game.

### 1.2 The goal

Point plinth at a folder of STL files. For each model, plinth chooses a pose, hollows it and drills drains when it is large enough, and adds supports sized from a model of peel load. It writes one ChituBox project file per model, with real, editable ChituBox supports. It shows, with pictures, where the result is weak and what easy edit would fix it.

Success means: at least as many prints succeed as with his ChituBox method, and ideally more; he can tell beforehand which ones might not; the treant becomes practical; and the whole process takes no longer than doing it in ChituBox.

### 1.3 Why this is a fresh start

A first attempt ran about three months and was deleted. Craig confirms that nothing it generated was ever printed. Its main mistakes shape this PRD:

- Hard limits (contact size, no fused contacts) were added late, as repair steps, and kept breaking. Here they hold on the final file and every step must keep them (section 6).
- Progress was steered by counts the project invented; each one misled. Here only the emitted file, the owner's judgement and real prints count (sections 6, 8).
- Thin physics overrode the owner's practice and gave over-supported output. Here physics is bracketed and his practice is the baseline (section 7).
- Work was judged on screen, never on the printer. Here a stage passes only on a successful print (section 8).

## 2. Users and workflow

One user: Craig, on a MacBook (Apple Silicon, 12 performance cores, 128 GB). ChituBox v1.4.0 on macOS; he updates when releases appear.

The workflow plinth must fit:

1. He collects a batch of STLs in a folder (units: millimetres). Kits (for example HeroForge) arrive as separate files: mini, base, familiar.
2. He marks any model he has pre-oriented by hand with the keep-up marker.
3. He runs plinth on the folder.
4. He gets one .chitubox file per model, plus a report with pictures.
5. He opens each file in ChituBox. He rotates and zooms in 3D and runs ChituBox's own checks (overhang detection, island report). He makes the easy fixes the report names.
6. He merges models into plates in ChituBox (drag and drop). The goal is one ChituBox file per batch.
7. He slices and prints. After wash and cure he removes supports with small wire snips, a hooked steel pick, a tapered steel pick, a rotary tool, or his fingers. Kit parts are glued after removal.
8. He paints all models with acrylics.

## 3. Scope

### 3.1 In scope for v1

- Input: a folder of mm STL files, processed model by model.
- Pose choice, with the keep-up marker as the only owner hint.
- Hollowing and drains for large models, decided together with the pose.
- Supports sized from a model of peel load.
- One .chitubox per model with editable ChituBox supports, readable by ChituBox v1.4.0 on macOS.
- A report per run: warnings with pictures, assumptions, easy fixes, and reported quantities.
- A complete, documented read/write library for the .chitubox format.
- An independent checker that runs on every emitted file by default.
- One printer: Saturn 4 Ultra 16K. Three resins the owner uses (Nova3D Water Washable ABS-like tough grey, Sunlu Water Washable ABS black, Anycubic Water Washable ABS-like 3.0 clear), treated as one material until prints show a difference.

### 3.2 Out of scope for v1 (the "later" list)

- Packing several models onto one plate file (he merges in ChituBox).
- Plinth's own slicer; variable layer height or exposure ("way down the list").
- Sending jobs to the printer over the network.
- Recognising what a region is (face, front, hand) from the mesh; this needs a vision model.
- Removing the supports from pre-supported models.
- Automatic splitting of an unprintable model into pieces.
- GPU acceleration.
- Other printers; Windows and Linux.

## 4. Functional requirements

Each requirement is testable. "Report" means the per-run report in the output folder.

### 4.1 Input handling

- **FR-1** Plinth accepts a folder of STL files (binary and ASCII) in millimetres and produces one output per readable file.
- **FR-2** Each file is an independent model. Plinth never merges files. A kit's parts are supported separately.
- **FR-3** A file that cannot be read gets a clear status line in the report and no project file. The other models continue.
- **FR-4** Existing cavities (owner ruling 2026-10-30). Plinth does not try to detect pre-hollowed models. The hollowing rule (FR-14) applies to the model's solid volume as it is, so a model that already has some cavities may be hollowed further. Plinth finds every sealed cavity, existing or new, and gives each the drains and vents it needs (FR-17).
- **FR-5** Pre-supported models: plinth does not try to detect them. In an STL, existing supports look the same as the model (owner, 2026-10-30), so every input is treated as model and supported as given. Feed it the unsupported version of each model.
- **FR-6** A model with the keep-up marker keeps its up direction. Plinth may turn it only about the vertical axis, to place it on the plate (owner ruling 2026-10-30). All other steps run as normal. Any one of four forms sets the marker (owner ruling 2026-10-30): a marker file beside the STL; a Finder tag on the STL; a name pattern in the STL file name; an explicit command-line flag. **Spellings (owner ruling 2026-10-30):** marker file `<name>.keepup` beside `<name>.stl`; Finder tag (the owner's existing tag) `plinth-no-rotate`; file name ending `.keepup.stl`; flag `--keep-up <file>`, repeatable. Git does not keep Finder tags, so files in the repository use the marker file or the name pattern. For each model the report states whether it was marked and by which form. A near miss (for example a tag `plinth no rotate`, a marker file whose STL is missing, or the first attempt's `.asdrawn` marker) is a warning, never silently ignored.
- **FR-7** The emitted model keeps the input's scale, every component, and its exterior surface, except for declared hollowing, declared drains and approved repairs. Approved repairs (owner rulings, extended 2026-10-30):
  - remove loose dust fragments and exact duplicate faces; drop a separate piece only when it fits inside one printer pixel across AND one layer high;
  - make the whole mesh strictly manifold (closed, no holes, no self-intersections), but only by a repair that keeps the shape. The output is always strictly manifold, so that it makes mathematical sense (owner ruling 2026-10-30);
  - keep legitimate pinch points such as touching fingertips, each made manifold by fusing the touching surfaces with a neck about 0.05 mm thick: enough to make the mesh manifold and a few printer pixels wide, too small to change the look (owner ruling 2026-10-30). H9 applies to support parts, not to model geometry, so the two do not conflict;
  - merge shells that touch or overlap into one before hollowing; a piece that touches nothing else is reported, not deleted (dust aside);
  - reduce mesh detail (decimate, merge near-flat faces) to speed processing, keeping thin features, separate parts and open passages, within a two-way distance limit: each surface stays within **\[Orchestrator starting value\]** 0.007 mm (half the smaller pixel) of the other. This is an error budget, not proof that the change cannot show; the checker also looks for changes in topology and in the sliced layers. If a model cannot be reduced within it, it is not reduced;
  - suspected sculpting defects, such as the thin flap on the treant's foot: plinth flags each one with a picture and a proposed fix. It removes a category of defect on its own only after the owner and the orchestrator have defined that category together and tried the fix on sample models (owner ruling 2026-10-30); until then, flag only. No other repair may be used to remove such detail.

  Repairs happen before planning. The checker compares the emitted model with the input STL, the recorded pose rotation and the list of permitted changes; the report gives their file hashes. The report shows each change large enough to see with a picture, and lists each smaller change in one line with its largest deviation.
- **FR-8** Plinth warns about features too thin to print reliably and says whether each looks like a modelling error (FR-7, last bullet).
- **FR-9** Plinth checks the fit in the build volume after rotation to the chosen pose, with supports: a model that fits as drawn can be too big on some axis once rotated (owner ruling 2026-10-30). Pose choice prefers poses that fit. Plinth warns when a model does not fit, or when its size suggests the units are not millimetres.

### 4.2 Pose

- **FR-10** Without the keep-up marker, plinth chooses the pose.
- **FR-11** Pose choice uses the owner's reasons, in the priority order of section 5, as far as geometry can show them: the model prints first; areas of fine surface detail face away from the plate, so supports do not collide with or scar them; thin protruding parts stand near vertical, where they support themselves; drains can sit on low-detail surfaces. Plinth does not recognise faces or fronts. The owner protects those with the keep-up marker.
- **FR-11a** Base pose (owner, 2026-10-30). A base prints with its underside toward the plate, so the marks land where they are hidden. It is tilted so that it mostly supports itself. By hand he tilts bases 45–60° from flat. Steeper needs fewer supports and less peel force, but puts more contacts on the partly visible rim and risks too few supports, so that the base peels off them. Flatter means more peel force, more supports and more resin. **\[Proposal\]** In automatic pose (stage 3 on), the angle is chosen per base by the same load model as the supports, keeping every contact on the underside where the tilt allows. In the first attempt, automatic pose mistook the stiffening ribs under these bases for detail to protect. General automatic pose must treat such underside ribs as support sites, not detail.
  - **Stages 1 and 2** (owner ruling 2026-10-30): plinth does not pose the bases. The orchestrator makes a new STL copy of each, tilted with the underside on the lower side of the tilted disc (toward the plate), and with the keep-up marker in an FR-6 form. The starting proposal is 65° from flat. Before any support work starts, he sees the tilted STLs and labelled pictures of the underside and rim, and confirms the angle looks right. Or he picks the angle in ChituBox, and the orchestrator rotates and saves the copies at that angle. The original is kept; the report records the rotation and both file hashes. The flat copies in test-data/models/as-drawn/ are not used.
  - **Round bases confirmed** (owner, 2026-10-30): the tilted Round 25, 50 and 100 copies at 65° from flat are approved, and sit in `fixtures/round/approved/`. This covers the Round bases only; the Morel base (stage 2) needs its own confirmation.
- **FR-12** The report states each model's pose and a one-line reason in plain words (for example "upright; thin parts vertical; drains on the underside").
- **FR-13** Every model is lifted off the build plate on supports (H4). No model surface rests on the plate (the long first-layer exposure swells it).

### 4.3 Hollowing and drains

- **FR-14** Plinth hollows a model only when the resin saved is worth the holes it needs: at least two holes in the surface (a drain and a vent), which cost less when they sit on hidden surfaces (owner ruling 2026-10-30). Other models stay solid.
  - **\[Proposal\]**, not yet calibrated: hollow when it saves at least 10 mL of resin and at least 30% of the model's resin; halve both figures when every hole can sit on a surface known to be hidden (not merely low-detail).
  - Before stage 5, the orchestrator compares the test with the owner's calls (2026-10-30): hollow the Awakened Tree, Giant Bat and Giant Toad; probably the Warhorse; the Rotting Warhorse is the boundary case; the rest solid. The Giant Toad Swallowing already has a large cavity; the test applies to its remaining solid volume, and its cavity gets drains (FR-4). If they disagree, it changes the test or asks him; it never relabels his calls. Measured 2026-10-29: the whole Warhorse holds 10.8 mL and the Rotting Warhorse 17.9 mL, so a test on resin saved alone may rank them the wrong way round.
  - The report states the resin saved (measured on the emitted model) and each hole's place. If a model that passes the test is not hollowed, or a cavity does not drain, plinth warns, and that model does not pass acceptance.
- **FR-15** Pose, hollowing and drain placement are decided together. A drain is never added after the pose is fixed without re-checking the pose.
- **FR-16** Drains sit on low-detail surfaces where possible (underside, inside a base). The report shows each drain in a picture.
- **FR-17** For each cavity, the report shows that in the print pose resin can run downhill from every part of the cavity to a drain, and that air can enter from a vent. "Downhill" is as the model hangs from the raised plate after printing, so resin runs back into the vat (the orchestrator's reading of the 2026-10-30 ruling). Chains of connected cavities are allowed if they end outside. A path is not proof of emptying: on hollowed stage prints the owner notes any resin left inside after his normal wash.
- **FR-17a** Print-risk check (mandatory, separate from H5). Plinth checks each cavity in print order and finds every cup with no adequate path for resin to the outside. A hole printed later does not relieve an earlier cup. Plinth adds that load only through a load-model treatment the report states. Otherwise it gives a located intervention or unprintable warning, resolved under 8.3; this never waives H5.
- **FR-18** No support passes through the bore below a drain hole. Thin supports may place their contacts right at the rim (owner, 2026-10-22), as long as they do not enter the bore.
- **FR-19** Supports inside a drained cavity may stay there; cavity ceilings are supported from below. They are never removed, so the tool-access and removal requirements (FR-25, 6.4) do not apply to them (owner ruling 2026-10-30). They still obey the load model, H2, H3, H9 and the drain-bore rule (H5). H1 and H6 do not apply there (owner ruling 2026-10-30): they limit scars and removal effort, which do not arise inside a sealed cavity.

### 4.4 Support generation

- **FR-20** Support amount and size come from a model of peel load, using the section 7 parameters. There is no "light" or "better" preset.
- **FR-20a** Before the first Round 25 is offered for printing, the load model states how a layer's peel area is measured, how load passes through printed model and supports to each support section, how supports share load, and how merged branches carry their combined load. These are assumptions to test: the unprintable ratio (P4) is the owner's limit, set from survival on a few geometries, not a measured breaking point, and may not hold for angled, bent or merged supports.
- **FR-21** For each model, the report states the parameter values used (including the active design ratio, P5); the largest area ratio in the emitted file and where it is; the areas treated as needing support; the areas judged self-supporting and why; and the uncertainty. Every load path the model cannot assess, or cannot hold at the design ratio, gets a located intervention or unprintable warning. These are claims to test by printing, not proof of printability.
- **FR-22** Each contact is the smallest that carries its modelled load, and never larger than H1. When one contact at the limit is not enough, plinth adds more contacts that do not touch each other.
- **FR-23** No two contact patches touch or merge (H2).
- **FR-24** Fragile parts (**\[Orchestrator starting values\]**; the owner delegated them, 2026-10-30; tuned from his removal notes). A part is fragile where its cross-section, taken square to its long direction, stays under 1 mm² over a length of at least three times its thickness. The report shows each one found and flags uncertain cases. On a fragile part, a contact's area is at most 10% of that cross-section, and never above H1. The aim is that the support, not the part, gives way during removal; this is untested, and area alone does not decide where a part breaks under bending. FR-24 is a target, not a hard limit. Where it cannot be met (for example, where the target contact area and the required load-bearing geometry cannot both be achieved), plinth keeps every hard limit, never changes the part, and gives an intervention warning. The report lists every contact on a fragile part with a picture.
- **FR-25** Contacts on or next to fragile parts leave room for one of his tools (snips, hooked pick, pointed pick) to cut them free without twisting the part. Accepted by print results and the owner's removal notes (6.4).
- **FR-25a** Kit parts: known glue faces are preferred contact sites, because the glue joint hides the scars (owner ruling 2026-10-30), for example the flat underside of a mini that glues to its base. Only a face the joined part covers fully counts, never a base's visible top. Geometry cannot prove a glue face, so the report marks such sites as inferred. Contacts there obey H1, keep clear of pins, sockets and edges that set the fit, and must not stop the parts fitting (6.4).
- **FR-26** Outside its contact-surrounding region, no support touches or enters the model (H6). Each contact's approach direction is chosen when the contact is placed. Overlap inside the contact-surrounding region is kept as small as the load allows and is reported.
- **FR-27** A support may stand on the plate, on the model, or on another support. Supports may merge freely near the plate; tool room matters only at the model end. The bases of supports that stand on the plate are joined by a "connected" raft (glossary; owner ruling 2026-10-30), sized from his profile (P17); the raft obeys H3 and H4 like any other support part.
- **FR-28** Plinth adds no support that its load model does not call for. The report shows each model's support density and the smallest contact spacing, so the owner can judge removal.
- **FR-29** Small islands that join supported structure within a few layers are left unsupported (P18). An unsupported island never counts as holding anything above it. The report lists each such island as an information warning with a picture, so that it can be matched against ChituBox's island report (8.3).
- **FR-30** Where a section 7 range is wide, plinth stays close to the owner baseline. Any departure from it cites a measured reason in the report.

### 4.5 Warnings and report

- **FR-31** For every readable model, plinth writes the work it can complete safely and a loud warning for each problem (subject to FR-42 and NFR-4). It never drops a problem silently. A model it cannot make printable gets a diagnostic file whose report lists every failed or unverified check; it cannot pass 8.3, and its supports still obey every construction limit. An input defect FR-7 cannot repair is kept and flagged.
- **FR-32** Each warning states what (for example "unsupported overhang"); where, as a marked region on rendered pictures in ChituBox-like colours from at least two views; why; and how big (area or length, how many supports were rejected there). Body-part names are used only when reliably known, for example from part file names or the owner.
- **FR-33** Each warning has one class: **intervention** (an easy fix is needed, or a risk must be accepted, before printing); **unprintable as is** (with a hint where splitting the model would help); or **information** (no action needed). Any warning may also be tagged "case to improve plinth".
- **FR-34** An intervention warning that can be fixed by hand names the easy fix: where to add a support in ChituBox. Plinth never suggests moving a support.
- **FR-35** Each model gets a short risk summary: the regions most likely to fail, the assumptions behind each, and how uncertain they are.
- **FR-36** The report can be read without a terminal (for example a page with pictures). Text is always paired with a picture of its region. There are no coordinates without a picture.

### 4.6 Output and run status

- **FR-37** One .chitubox file per readable input model, named after the input file's stem, `.keepup` included. Names are unique within a run; any clash is settled by a documented, deterministic tiebreak. No output ever overwrites another input's output.
- **FR-38** ChituBox v1.4.0 on macOS opens every file, shows the model and its supports, and slices it, with no error.
- **FR-39** Supports are native ChituBox supports: he can select, edit, delete and add them in ChituBox, and they look distinct from the model.
- **FR-40** Files merge by drag and drop into one ChituBox project without losing supports.
- **FR-41** The .chitubox library is a deliverable. It reads and writes files with several models, several plates, and every support type ChituBox writes. Its format documentation cites, for every claim, a native file that shows it.
- **FR-42** Cancel, crash, input errors and disk errors give a clear per-model status. Models already finished are kept. No file exists that looks complete but is not (each file is written whole or not at all).

## 5. Priority order when goals conflict

The owner's ranking (round 4), highest first:

1. **It prints.** Nothing falls off the supports; no squashed unsupported regions; no sag; no layer separation.
2. **The model is usable after support removal.** No chunks out of a detailed area; no knobbled blades; no broken fingers; supports not packed too densely or attached too solidly.
3. **The whole process does not take longer than doing it in ChituBox would have.** This includes his review and edit time, not only plinth's run time.
4. **Looks.** Scars on visible surfaces. Hidden scars do not matter unless they harm the part (for example a hole through a base that shows on top).
5. **Resin use is not excessive.** Print time (mostly total height) ranks with resin, at the bottom.

How it applies:

- Hard limits (section 6) are never traded. Priorities choose among designs that meet every hard limit.
- If a region cannot print within the hard limits, plinth does not break a limit. It writes the file and an intervention or unprintable warning; the owner decides.
- A pose that prints more reliably beats one with cleaner detail areas (1 over 4).
- A sparser layout that is easier to remove beats one that saves resin (2 over 5).
- Fewer manual edits count under item 3: a file he must edit heavily fails item 3 even if it prints.

## 6. Hard limits, targets and reported quantities

### 6.1 The rule

The owner's principle (ruling 2026-10-28): a limit is built in where the geometry is created. It is not met by a later repair pass. Ideally there are no repair passes at all; a minor one is a rare exception, never the main approach (section 10).

The acceptance rule, in output terms:

1. Hard limits apply to the final emitted geometry.
2. Every operation that can affect a limited quantity must preserve the limit. No step may break a limit and rely on a later step to fix it.
3. The independent checker verifies H1–H7 and H9 on the emitted file. H8 is a separate record of ChituBox opening and slicing that exact file. Plinth's internal plan is never evidence. A check that cannot be completed is reported as unverified, never as passed.

### 6.2 How the file is measured

- **Contact patch.** The connected area of model surface that a support touches or enters, measured on the model surface in mm² (owner ruling 2026-10-30: area, not diameter). It is computed from the model and support geometry that ChituBox v1.4.0 actually uses, never from a support's diameter field. Overlapping parts of one support count once; different supports stay separate (H2). Feet on the model and incidental touches count. The checker states each area with an uncertainty bound; a patch passes H1 only if the upper end is within the limit, and an unmeasured patch fails. The sampling resolution (**\[Orchestrator starting value\]** 0.01 mm) must be shown to meet that bound on native examples. This is a measurement rule, not a prediction of the printed scar.
- **Which geometry ChituBox uses** is not yet known: a support record holds end points and radii, and ChituBox may rebuild a whole ball where plinth clipped it (P14). Native samples (section 12) settle this before the first print. Until then, plinth uses only contact shapes whose native form is known.
- **Contact-surrounding region.** The end of a support next to its contact: the contact ball (the small sphere that touches the model), the neck just behind it, and the tip cone (the cone that widens from the neck to the support's column). It ends where the support stops getting closer to the model (owner, 2026-10-21: once a support leaves its landing area, it never comes closer to the model than it was there). It is never longer than a set distance from the contact centre (3.0 mm, the tip cone length in his Heavy profile; owner ruling 2026-10-30: a safe starting value, to shorten or lengthen from print results). A foot that stands on the model has the same kind of region, measured from its contact. The length matters because it sets how hard the support is to remove, and how likely it is to stick to the model when the print is slightly over-cured.
- **Fused.** Two patches that touch, or are closer than the measurement resolution.
- **Incidental contact.** Any support touching the model outside a contact-surrounding region. Every incidental contact is reported and breaks H6. It still counts as a patch for H1 and H2. Any exception (for example his earlier acceptance of a far-wall touch in a narrow crevice) is the owner's call, case by case, as such cases arise (owner ruling 2026-10-30).
- **Print-order continuity.** Layer by layer from the plate up, every piece of support in a layer connects to the plate, or to model or support printed in that layer or below. Nothing relies on geometry printed later. There are no circular load paths.

### 6.3 Hard limits (checked in the emitted file)

| ID | Hard limit | Source | Measured as |
| --- | --- | --- | --- |
| H1 | Outside sealed cavities (FR-19), every contact patch ≤ 0.50 mm² (the area of the 0.8 mm round mark from round 4; owner ruling 2026-10-30: a starting value, to be tightened or loosened if test prints show it is too strict or too loose), including support feet that stand on the model. Feet may be split to share load. | Owner, round 4: larger marks are hard to separate, damage detail that shows after painting, and are hard to remove | Upper bound of the largest patch area per model (mm², 6.2) |
| H2 | No fused contacts | Owner ruling 2026-10-25: any merged mark is a defect, because a tool must fit between marks. No minimum gap beyond this is ruled (he declined one) | Fused pairs: 0 |
| H3 | Print-order continuity for every support | Physical necessity | Unconnected or later-reliant pieces: 0 |
| H4 | Model lifted on supports; no model surface rests on the build plate, a raft, or the base of a support that stands on the plate. Support feet that stand on the model stay allowed (H1–H3, H6) | Owner printing experience (first-layer swell) | Lowest model point above the plate ≥ his profile's lift height, 5.0 mm (P17; **\[Orchestrator starting value\]** as the minimum) |
| H5 | Every finished cavity drains and vents in the print pose (FR-17); no support in a drain bore (FR-18) | Owner, round 2 | Cavities failing FR-17: 0; supports in a bore: 0 |
| H6 | Outside sealed cavities (FR-19) and outside its contact-surrounding region, a support keeps a gap from the model of at least 0.3 mm | Owner printing experience (P16) | Smallest gap outside contact-surrounding regions; incidental contacts: 0 |
| H7 | Model fidelity (FR-7) | Owner workflow: he prints what he chose | Emitted model vs input STL, recorded pose rotation and list of permitted changes: no other change |
| H8 | The file opens, shows its supports and slices in ChituBox v1.4.0 | Owner, round 3 | A record that this exact file was opened, inspected and sliced in ChituBox. A library round trip is not an H8 pass |
| H9 | No load-bearing support part (neck, rod or wall) thinner than 0.1 mm, measured across the part, including narrowed joins and clipped sections. The tip of a ball, a mesh edge or a one-layer sliver is not a part. The rule is tested on native examples before the first print | Owner ruling 2026-10-30: a default for this printer (about 7 pixels in X, 5 in Y, 2 layers at 0.05 mm); may change with print results | Thinnest such section per model, and where |

**Not hard limits (starting values, tuned from print results):** the fragile-part contact limit (FR-24); wall and drain sizes (P19, P20). H1, H6, H9 and the 3.0 mm region length are hard limits whose values are also starting values; they change only by owner ruling.

### 6.4 Physical acceptance (observed on prints, not in the file)

- Print success (8.3 definition).
- Scars and removal damage, judged by the owner after removal.
- Function after removal: a base stands without rocking on support remains; kit parts still fit together with no nubs holding the joint apart. Any extra smoothing and its time are recorded (8.4).
- Tool access near fragile parts: removal is the owner's subjective judgement as he takes supports off. After each print he notes any place that was especially hard or especially easy (owner ruling 2026-10-30); those notes are recorded per model. No formal demonstration.

### 6.5 Targets and reported quantities

Targets (traded in section 5 order, inside the hard limits): print success; easy removal; low overlap inside contact-surrounding regions; scars away from detailed areas; fewest manual edits; low resin; low print height.

Reported quantities (stated, not limited): resin volume of model and of supports (mL); print height (mm); run time per model and per step (s); peak memory; contact count, largest patch and smallest spacing; largest area ratio; thinnest emitted hollow wall and narrowest cavity passage; warnings by class; for comparison prints, the same numbers for the ChituBox copy. Resin is never a reason to break a hard limit.

Every number in a report states its unit, what it counts, what it does not count, its denominator, and the file it was measured from.

## 7. Physics knowledge as parameters

Plinth's physics is thin. These are bracketed, replaceable parameters, not laws. Each keeps its range (never a midpoint guess) and its evidence class. A setting from his ChituBox profile, or a value that worked for him in some cases, describes that case only.

Evidence classes: **Measured** (on this printer: a test print or the owner's read of one); **Owner experience** (his printing, no test print); **Owner preference**; **Orchestrator starting value** (chosen by the orchestrator; not evidence); **Convention/literature** (vendor, papers, ChituBox settings); **Unmeasured** (an estimate; must not decide alone). "Changes by": **Obs** = a new observation on a print the owner judged credible beforehand; **Ruling** = an owner ruling; **Config** = verified vendor data or his exported profile.

Test-print conditions: the plates were sliced with the Sunlu black profile (0.05 mm layers, anti-aliasing on). Plate 5's print conditions were not recorded. Plate 6 was printed 2026-07-30 at about 30 °C with Sunlu black resin, his only black resin (owner, 2026-10-30).

Print settings to plan for (owner ruling 2026-10-30): the printer's default layer height and ChituBox's default print profile. Resin is not a planning input for now; it may become one later. His resins are all water-washable, tough, ABS-like types, a narrow band of the resin range, and that band is the one plinth targets. Plinth treats them alike until prints show a case where the resin makes a difference.

| ID | Parameter | Value or range | Evidence class | Conf. | Changes by |
| --- | --- | --- | --- | --- | --- |
| P1 | Printer | Saturn 4 Ultra 16K; 15120×6230 px; pixel 14 µm (X) × 19 µm (Y); build 211.68×118.37×220 mm; tilt release, front lifts first | Convention (vendor) + owner experience | High | Config |
| P2 | Process | Layer height 0.05 mm, his default (owner ruling 2026-10-30). ChituBox applies the layer height only when it slices, so files do not store it. Other values: ChituBox's default print profile (read from the owner's first v1.4.0 sample file, section 12). Test prints so far used the Sunlu black profile: 0.05 mm layers; 1.2 s normal, 3×15 s bottom exposure. The three resins behave alike in his experience | Owner ruling (layer height) + measured (setup) + owner experience | High | Ruling, Config |
| P3 | Load model | Peel is the load. Weight is negligible except for very thin supports or heavy solid parts. Suction is minor on a tilt printer, except in a closed cup (FR-17a) | Owner experience | Med | Obs |
| P4 | Unprintable ratio | Active value: 40 (owner ruling 2026-11-10). It is the owner's limit, not a measured breaking point. Supports survived above the old limit of 25: ChituBox Better supports on Round 25 at an additive ratio of about 33.5 (2 of 2 copies, 2026-11-10 print, `docs/experiments/r25-side-by-side-results.md`), and the CAL-1 K-60 coupon at 60.15 (2 of 2). Losses above 25: plate 6 lost cylinder support M2 at 30.48 and its K-60 twin M1 at 60.15 (one specimen each; black resin, printed 2026-07-30; `reference/experiments/plate-06-results.md`). The H2 plate (2026-08-04; resin not recorded) lost heads, with head-loss boundary X* in (25.00, 36.00), and all four CM controls at X = 36 died in print (`reference/experiments/h2-plate07-results.md`). The old 25 came from plate 5 (cone supports 25.00–25.34; resin and print date not recorded; six families, three cap diameters). Its 25.00 end is one specimen the owner described; its 25.34 end is inferred from his row-4 counts, not read. Plate 6 measured cylinder supports at (19.91, 30.48), one specimen at each end. The evidence above 25 is mixed: older losses (plate 6 on black resin; H2 unrecorded) against newer survivals on ABS-like clear (R25, CAL-1). Resin is one possible cause of the split, not an established one; CAL-2 is to settle it. The owner trusts the newer prints more and chose 40 to keep margin below 60 | Owner ruling on prints | Med | Ruling (informed by prints) |
| P5 | Design ratio | Active value: 12 (owner ruling 2026-10-30: "start with 12 and see"; reaffirmed 2026-11-09 after CAL-1). CAL-1 (2026-11-09): ChituBox Light failed 4/4 on Round 25, as the full-area load law predicted; thin strips showed no advantage over blocks at index about 97. The owner kept 12 as the conservative choice even if this resin might tolerate thinner or sparser supports: "let's play it safe rather than have failures on future prints" (ruling 2026-11-09). He recalls that 12 looked like over-support last time, which is why he relaxed it to 18 on 2026-09-30, but many other problems may have caused that. Prints of plinth's own supports show whether 12 over-supports; if it does, moving toward 18 is the expected next step. Evidence the other way: heads were lost at 9 on one plate (single specimens, cause not certain), and another way of measuring the held area puts that boundary at 11–14, which includes 12 | Owner ruling on thin measurement | Low | Ruling (informed by prints) |
| P6 | Load sharing | Several supports under one area held at least the sum of their parts, tested up to 6, in one geometry | Measured | Med | Obs |
| P7 | Failure site | Supports snapped at the narrowest section just above the contact; supports that widened slowly above the contact snapped there | Measured (owner read) | High | Obs |
| P8 | Unsupported flat shelf reach | No loss ≤ 2.13 mm; first loss 2.59 mm; all lost ≥ 4.67 mm (readings of the same print differed by up to ±0.8 mm) | Measured | Med | Obs |
| P9 | Unsupported span | About 3 mm between supports for a 0.3 mm thick section | Measured (low-res photo) | Low | Obs |
| P10 | Smallest tested wall | 0.10 mm printed full height; smaller not tested | Measured | High at 0.10 | Obs |
| P11 | Overhang angle | 45° is ChituBox's setting; never measured here; printability treated as continuous in angle | Convention | Low | Obs |
| P12 | Unbraced column slenderness | Unmeasured. His view: long thin unbraced posts will not print; thin rods beside thick columns get braced to them | Unmeasured + owner experience | Low | Obs |
| P13 | Thin tips | 0.1 mm diameter tips worked for him on short thin runs; not a general minimum | Owner experience | Med | Obs |
| P14 | Contact shape | His profile: a sphere along the surface normal. The contact area needed governs; depth follows. A ball deeper than the surface is clipped, not pulled back, if ChituBox keeps the clipped shape (6.2; not yet known) | Owner preference + practice | Med | Ruling |
| P15 | Contact spacing in his profiles | Tip distance setting 4 / 6 / 8 / 8 mm (Ultralight / Light / Middle / Heavy). A setting, not proof that unedited output prints, and not measured spacing | Owner practice (settings) | Med | Config |
| P16 | Support-to-model gap | True gap 0.3 mm held in his prints ("could go lower"); ChituBox placement margin setting 0.6 mm | Owner experience / convention | Med / Low | Obs, Ruling |
| P17 | Lift and base | Always lift; his profile: lift 5.0 mm, raft 0.2 mm, base 2.2 mm dia × 1.0 mm | Owner experience / settings | High / Med | Ruling, Config |
| P18 | Small islands | No support if ≤ 25 printer pixels in both directions (owner ruling 2026-10-30), little growth, and joined to supported structure within about 2–3 layers | Owner experience | Med | Obs |
| P19 | Hollow wall | 2.5 mm, his ChituBox setting, which works in practice (owner ruling 2026-10-30: a starting value) | Owner experience (hand prints) | Med | Obs, Ruling |
| P20 | Drain bore | 2.0 mm diameter drain at each cavity's low point in the print pose (FR-17), plus at least one vent (**\[Orchestrator starting value\]** same diameter, at the cavity's high point). His ChituBox setting, which works in practice (owner ruling 2026-10-30: a starting value) | Owner experience (hand prints) | Med | Obs, Ruling |
| P21 | Front-to-back plate position | No effect found; at most about 1.5× | Measured (a bound) | Med | Obs |
| P22 | Peel pressure (literature) | Exponent 0.24–0.46; 9–19.6 kPa at 314 mm² (other resin, film, machine) | Literature | Low; must not decide | Obs |

Rules:

- An empirical range changes only from an observation on a print the owner judged credible beforehand and scored afterwards. The change records the print, its conditions, and the new range and class.
- A preference changes only by an owner ruling. Printer and process settings change only from verified vendor data or his exported profile.
- Any constraint plinth derives from these parameters (for example a spacing computed from P5 and P8) is a proposal until a print supports it.
- Where a range is wide, plinth uses the owner baseline, not the cautious end of the range.
- Each print records: model, file hashes (generated and edited), pose, resin and lot, layer height, exposure, tank temperature, plate position, film age, and the outcome per region.

Plan to firm them up (only with prints the owner judges credible):

1. The first data come from the staged miniature prints themselves (section 8), each printed beside a ChituBox copy.
2. A test print is proposed only when its result would change a decision on a staged model, an AI reviewer from a different AI model family than the designer's has checked that it is credible (likely to print as designed and give a readable answer), and it can share a plate with a real print. Never one that is doomed to fail (owner, round 3).
3. Candidates, cheapest and most decisive first, are in docs/lessons/lessons-physics.md section 4. The design ratio (P5) and slenderness (P12) matter most. Once stage 3 prints exist, the orchestrator picks which to measure first and asks before printing (section 11).

## 8. Success metrics and acceptance

### 8.1 First milestone: Round 25

The first thing built is a printable Round 25 base (minis-to-process/Round 25.stl): solid, as a tilted keep-up copy (FR-11a), with native editable ChituBox supports, passing the checker limits that apply (H1–H4, H6, H7, H9) and the ChituBox open-and-slice record (H8), with a report and review pictures. Before any support work starts, he has confirmed the tilted copy's angle (FR-11a): 65° from flat, on 2026-10-30. Before it is offered for printing, FR-20a is met, and the section 12 samples have settled which contact geometry ChituBox uses (6.2). Then, as soon as the file opens and slices in ChituBox, the owner is asked for his judgement. If he finds it credible, he prints it. This milestone does not wait for automatic pose, hollowing, full format coverage, or speed work. Round 50 and Round 100 then complete stage 1.

### 8.2 Staged targets

Each stage passes before the next starts. Failure on a simple stage is treated as a flaw of principle, not a value to tune.

| Stage | Model(s) (minis-to-process/) | Why |
| --- | --- | --- |
| 1 | Round 25 (milestone 8.1), Round 50, Round 100 | Simplest support job, as tilted keep-up copies whose angle he confirms before support work starts (65° confirmed 2026-10-30 for the Round bases; FR-11a); a failure here means a basic flaw |
| 2 | Morel base | Slightly fancier base; a tilted keep-up copy like stage 1 (FR-11a), so stage 3 stays the first automatic pose |
| 3 | Bat Cat Owl | Small, fairly simple mini; first automatic pose |
| 4 | Deinonychus | Mid-size; thin parts test the contact limits |
| 5 | Giant Bat | Large enough to hollow; pose, hollowing and drains together |
| 6 | Awakened Tree (the treant) | Complex hollowing, difficult crown; the reason for the project; the hardest speed test. ChituBox takes over 5 minutes on it but about 3 s on the Giant Toad, which has more triangles (459,072 against 429,120), so triangle count is not what makes it hard. Entry gate below |

**Stage 6 entry gate** (owner, 2026-10-30). The treant is attempted only when the earlier stages give good reason to expect success. Before it: (1) every step's time and peak memory have been measured on earlier-stage models and larger test cases that vary what drives each step (triangle count, size and height, cavities, narrow gaps, supports produced), separating a finer mesh from harder geometry. The results explain the growth and predict that the treant meets 8.5 and NFR-4. A curve over triangle count alone does not pass. (2) Pre-processing (FR-7 repairs and detail reduction) is in place and shrinks large meshes without visible loss where it safely can. Data structures and algorithms are chosen to handle meshes larger than the treant, since more complex models will come.

A stage passes only when the owner accepts a successful print (8.3) of each of its models, made from plinth's generated file with at most the resolved interventions of 8.3. A failed print does not pass. A waiver does not pass: the owner may waive a print only to let a named development task proceed. **\[Proposal\]** When stage 1 and stage 2 files are both credible at the same time, they may share one plate. This is an exception to "each stage passes before the next starts", and the owner agrees to it each time.

### 8.3 Credible to print, and a successful print

Before the owner is asked to print, all of these hold:

1. The independent checker reports zero violations of H1–H7 and H9 in the generated file, and none unverified.
2. The report meets FR-20a and FR-21: the active design ratio (P5, changed only by owner ruling), the largest area ratio in the emitted file and where it is, and a warning on every load path the model could not assess or hold. Choosing a design ratio does not by itself pass this item.
3. No "unprintable as is" warning remains unresolved. Each intervention warning is fixed (an easy fix in ChituBox) or explicitly accepted by the owner as a stated risk. Accepting a risk never waives a hard limit.
4. If he edited the file, the checker runs again on the edited file, and the edited file passes before printing. Both files are kept. Edits never count as the generator meeting its limits: the generated file must pass on its own.
5. The exact file to print, with any edits, is opened and sliced in ChituBox (H8). Its island report shows no islands beyond those the report lists (FR-29) or warned about and that are resolved or accepted.
6. The orchestrator has opened the output and its pictures itself and checked the headline numbers against the file.
7. The owner has reviewed it in ChituBox and judges it credible.

A successful print: nothing fell off, no squashed or sagging regions, no layer separation, supports removed without damage to thin parts or detailed areas, and the part works after removal (6.4).

### 8.4 Side-by-side comparison and counting

He imports the plinth file into ChituBox and adds a copy of the same model supported his usual ChituBox way to the same plate. One print run. Recorded per model, for both copies: printed or failed (and where); removal ease and damage; extra smoothing and its time; manual edits and their time; resin; print height; run time.

Counts, always by stage and in total:

- Prints attempted, successful, failed, and waived.
- Failures at a spot plinth warned about, and failures at a spot it did not (unwarned).
- Warnings that needed action, and warnings on regions that printed fine without action. A blanket of warnings does not prove prediction.

The comparison is evidence about the models printed, not about other models.

### 8.5 Speed

Plinth is no slower than ChituBox, ideally faster. Time is measured from invocation until all output files, checks and review material are written, on his laptop. He often uses the laptop during runs, so each timing records the other load on the machine, and a timing distorted by that load is marked and repeated (owner ruling 2026-10-30). The report also gives each step's time. His ChituBox times (auto-orient + magic support): Bat Cat Owl 1–2 s; Giant Toad about 3 s; Giant Bat about 5 s; Awakened Tree just over 5 min, plus a couple of minutes to hollow.

- Before a numeric limit is set for a model, its ChituBox baseline is measured for the same steps (orient, support, and hollow where plinth hollows). Missing today: the Round bases, the Morel base, Giant Bat with hollowing, and an exact treant time. The orchestrator measures them, asking before it drives ChituBox on his Mac.
- The treant must be dramatically faster than ChituBox: at most 120 s on the clock above (all outputs, checks and review material written), against ChituBox's roughly 7 minutes for orient, support and hollow (**\[Orchestrator starting value\]**, 2026-10-30; the owner may change it). It is the one limit set before its baseline is measured; it is checked again once the exact treant time is measured.
- His review and edit time per model is measured against the same ChituBox workflow on the same models (8.4).
- Stages may pass on physical proof before speed work. v1 acceptance requires every speed limit.

### 8.6 v1 acceptance

**Owner ruling 2026-10-30, to adjust as issues come up:** v1 is accepted when all six stages have passed; every speed limit in 8.5 is met; and the other 29 models in minis-to-process/ (37 in all, 8 of them staged) have been run and reviewed, with only easy fixes needed. Across all comparison prints, the plinth copies succeed at least as often as the ChituBox copies, and there are no unwarned failures.

## 9. Non-functional requirements

- **NFR-1 Speed.** Section 8.5. Run time per model and per step is in every report.
- **NFR-2 Determinism.** With the same plinth version, inputs and settings, the output is the same on every run, at any thread count.
- **NFR-3 Cancel and progress.** Cancel stops a run promptly (**\[Proposal\]** within 1 s) and behaves as FR-42. Progress shows which model and which step.
- **NFR-4 Memory.** Memory is the peak of the whole plinth job and its child processes (generation, checking, rendering). Hard ceiling: 100 GB for the job, whatever the number of parallel workers (owner ruling 2026-10-30: his laptop has 128 GB; above about 80 GB his other work suffers). Target: far below that. ChituBox peaks at about 1.7 GB on the Giant Toad, so **\[Orchestrator starting value\]** plinth stays at or below 4 GB per model on the largest current models (under 500,000 triangles). The report states peak memory per model and per job. Going over 100 GB is a defect. A miss of the 4 GB target is reported and investigated; the target changes only by a recorded decision.
- **NFR-5 Test against the real consumer.** Format tests use only files ChituBox wrote. Files written by plinth or built by hand are never ground truth. The owner opening and slicing the file in ChituBox is the final format check.
- **NFR-6 Independent checker.** It shares no code or types with the generator. It reads the .chitubox file and, for H7, the input STL, the recorded pose rotation and the list of permitted changes (file hashes in the report), and measures from them, never from the generator's claims. It runs on every output by default and can be turned off only by an explicit, logged opt-out. Each rule has a deliberately broken file, written through plinth's own production writer, that the checker must reject.
- **NFR-7 No invented metrics.** Every target or count used to steer or report names its source: an owner ruling, a physics derivation, a measurement of the emitted file, or a labelled orchestrator starting value. A starting value states its reason and is never presented as measured. Each states what it counts, what it does not, and its denominator. "Files emitted" is never a success measure.
- **NFR-8 Clean checkout.** A fresh clone builds and runs the tests with no local scratch files.
- **NFR-9 Thread limit.** Parallel steps are designed to use all available cores, and a command-line flag caps the number of threads (owner ruling 2026-10-30). During development the cap defaults to **\[Orchestrator starting value\]** 8 threads, leaving headroom because he uses the Mac at the same time.

## 10. Implementation constraints (not acceptance criteria)

Owner-directed:

- **Highest priority: a fast development loop** (owner ruling 2026-10-30). The first attempt was badly held back by cycles of up to an hour to build one change, run its tests and check the results. That must not happen again. The loop is designed and measured before real code. A skeleton (read an STL, write a .chitubox, run the checker) meets **\[Orchestrator starting value\]** budgets: edit + targeted tests ≤ 15 s; edit + Round bases end to end with the checker ≤ 90 s.
- Rust. If the skeleton cannot meet the loop budgets, propose a switch of language (TypeScript on bun, or Go) to the owner before real code is written (section 11).
- Build each limit in at the step that creates the geometry (ruling 2026-10-28). Ideally there are no repair passes. A minor repair pass may be needed, but it is never the first approach (owner ruling 2026-10-30); each one is named in the report with its reason, and the final check still verifies every limit.
- The old ChituBox reader/writer may be restored from the archive (owner permission, round 4), cautiously, reviewing each choice (section 12).
- Performance (owner ruling 2026-10-30): design for batching, efficient parallel work across the 12 performance cores, cache locality, and Apple Silicon vector instructions (SIMD/NEON). A GPU possibly later (round 3).
- Batch-first data flow: no per-item call in a hot path (his standing rule).

**\[Proposal\]**:

- Contacts follow his profile's shape (P14).
- Style defaults, from comments he made while reviewing pictures in the first attempt: angled tips, a short neck between contact ball and tip cone, tree-shaped merging of supports, and braces from thin columns to thick ones. Plinth may use them; they are not acceptance criteria. He needs to say something only if one bothers him in a review picture.
- Speed changes that alter output are accepted by a similarity check on the emitted file, not by byte identity.
- One default path, which is the tested path; no feature switch without a named owner and a removal date.
- The checker lives outside the generator's code base.

## 11. Working method

The owner gives high-level instructions; the work proceeds without him. The orchestrator polices its own cadence: if it asks too often, it scales back; if it drifts too far before a course correction, it asks sooner.

What the orchestrator reports:

- What he can open, what it does, what is broken. Plain English, short sentences, no internal codes or abbreviations.
- Pictures in ChituBox-like colours, before and after side by side, caption on the image, region marked. Body-part names only when reliably known. No pictures of changes too small to see. No interim snapshots he did not ask for.
- Numbers as 6.5 requires, with the previous value. Worse news first.
- Before any report, the orchestrator checks the headline requirement in the emitted file itself. It never relays an agent's claim unchecked, and it checks claimed commits against git.

When it asks the owner:

- Before spending resin (any print or test print). There is no fixed print budget; this is a hobby, so each request names the decision the print will settle and asks for no print that settles none (owner ruling 2026-10-30). Test prints may include specimens built to break at set steps, but only when the steps are expected to straddle the failure point, so that some hold and some break (owner ruling 2026-10-30).
- Before changing a hard limit, the priority order, or scope.
- When a new owner ruling contradicts a design decision (first question: which decision does this overturn?).
- When one of his rulings appears to need review. Craig is not infallible: the orchestrator raises it with him, with its reasons for a change (owner ruling 2026-10-30).
- Before switching language (section 10).
- Before operating ChituBox or any other app on his Mac by screen control, because it takes over a computer he may be using (owner ruling 2026-10-30). Use it only where it helps; ChituBox is not easy to drive this way.
- When a stop rule fires and the read-only review recommends a change of direction.

Technical choices inside this PRD are the orchestrator's to make.

Stop and step back. Any one of these stops new work, triggers one read-only review by an AI from a different AI model family than the one that built the code, and produces a plain written review:

- The owner expresses frustration with direction or speed.
- Code growth or repeated effort with no stated product benefit.
- **\[Proposal\]** Two misses in a row on the same kind of defect, or three review rounds on one change.
- **\[Proposal\]** Run time on a standard model rises more than 25% with no named cause, after checking for other load on his machine.
- A work report disagrees with git, the file, or the build.

Other rules from the first attempt:

- When a new limit arrives, first ask which step creates the quantity, and put the limit there.
- Before each task, decide what decision its result will change; skip work that changes none.
- **\[Proposal\]** At most three concurrent writing tasks; work branches deleted when merged; no parked branches kept as reference.
- Full runs of large models only at stage ends, never per change. One review round, real defects only.
- Owner rulings update this PRD in place (replace, do not append).

## 12. Reuse policy

- No code from the first attempt, with one owner-permitted exception: the old ChituBox reader/writer may be restored from the archive (the git bundle) and adopted cautiously. Every choice in it is reviewed before adoption. Known points to review: it was pinned to ChituBox Basic 1.3.0 (he now runs 1.4.0), and several format fields are unknown (reference/formats/chitubox-format.md section 8).
- Everything in reference/ is facts-only background. Its vocabulary, policies and numbers are not requirements unless this PRD restates them.
- The owner makes the ChituBox sample files, all from v1.4.0 (owner ruling 2026-10-30). They go in test-data/chitubox-native/samples/; that folder's README sets each file's name and contents. Each sample differs from the one it builds on by one change, so a file comparison shows which fields that change touches.
  - **First, for the Round 25 milestone (made 2026-10-30):** 01 plain (Round 25 imported alone, top-down as drawn, no supports); 02 lifted by hand, no supports (how position is stored; if ChituBox dropped the model back to the plate, record what it did); 03 one support of each kind ChituBox offers (how each native support type is stored; its "no raft" setting still makes ChituBox's plain default mat); 04-connected and 04-hex = 03 with the "connected" raft and with the hex raft; 05 two plates, each with a supported model and a raft; 06 (optional) = 01 flipped top-up, to show how a rotation is stored.
  - **After plinth's first output:** a plinth-written file with a clipped contact ball, opened, saved and sliced by him (whether ChituBox keeps the clipped shape, 6.2); and that output edited and saved by him (what ChituBox rewrites).
  - **Later, for downstream features:** an asymmetric model rotated only about the vertical (automatic pose); several separate models in one file and several plates in one file (merging); a project with non-default print settings (deferred: plinth plans for the default profile).

## 13. Open questions

None open (2026-10-30). New questions are added here as they arise, ordered by what they unblock, each naming who decides.
