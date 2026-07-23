# Volume-accounting audit: 12.8396 ml (engine) vs 9.99 ml (ChituBox)

Craig's hypothesis: our engine double-counts overlapping resin volume —
rod-rod overlap where members share a junction, ball-rod overlap where a
joint ball sits on a member. Settled below by measurement, not inference.

**Verdict up front: hypothesis CONFIRMED, and it is the dominant effect.**
The engine's reported 12.8396 ml is a **sum-of-independent-primitive-volumes**
computation with zero overlap correction. The measured **union** (true
printed solid volume) of the same 1625 members + 1102 balls is **8.74 ml**
at pitch 0.10 mm (converged across three pitches — 0.40/0.15/0.10 mm — all
agreeing to ±0.3%). That is a **32.0% overlap** (4.10 ml of the reported
12.84 ml is volume claimed by more than one primitive). Of that overlap,
**75%  (3.08 ml) is member-member (rod-rod) junction overlap** and **25%
(1.02 ml) is ball-rod overlap** — both halves of Craig's hypothesis are
real and rod-rod dominates.

---

## 1. Code review — how member_ml / ball_ml / by_kind_ml are computed

**`scripts/support_placement/tree.py:515-537`** — `volume_report(members, balls)`:

```python
515  def volume_report(members, balls):
...
520      by_kind = {}
521      member_ml = 0.0
522      for m in members:
523          p0 = np.asarray(_get(m, "p0"), float)
524          p1 = np.asarray(_get(m, "p1"), float)
525          r0 = float(_get(m, "r0"))
526          r1 = float(_get(m, "r1"))
527          L = float(np.linalg.norm(p1 - p0))
528          v = np.pi / 3.0 * L * (r0 ** 2 + r0 * r1 + r1 ** 2) / 1000.0
529          member_ml += v
530          kind = _get(m, "kind")
531          by_kind[kind] = by_kind.get(kind, 0.0) + v
532      ball_ml = 0.0
533      for b in balls:
534          r = float(b["r"] if isinstance(b, dict) else b.r)
535          ball_ml += 4.0 / 3.0 * np.pi * r ** 3 / 1000.0
536      return dict(member_ml=member_ml, ball_ml=ball_ml, total_ml=member_ml + ball_ml,
537                  by_kind_ml=by_kind)
```

This is a plain Python loop that sums the **closed-form frustum volume**
`pi/3 * L * (r0^2 + r0*r1 + r1^2)` for every member independently, and the
**closed-form sphere volume** `4/3 * pi * r^3` for every ball independently.
There is **no geometry comparison between primitives anywhere in this
function** — no bounding-box test, no distance test, no boolean/CSG step.
Each member and each ball contributes its own analytic volume regardless
of whether another primitive already occupies the same physical space.
**Confirmed: members are summed as independent frusta and balls are added
on top with no overlap correction.**

The members/balls lists fed into this function come from
**`scripts/support_placement/engine.py:93-102`** (`live_member_lists`,
called at `engine.py:193` and `engine.py:263`) — `joints.collect_members(live)`
for members, `joints.compute_joint_balls(live, plate_z, cfg)` for balls —
then **`engine.py:200`** / **`engine.py:310`**: `volume = tree.volume_report(members, balls)`.
This is exactly the code path that produced the pickled `engine_run_state.pkl`'s
`volume` dict; re-running `tree.volume_report` on the pickle's `members`/`balls`
reproduces `member_ml=11.490088468707665, ball_ml=1.3495416727466585,
total_ml=12.839630141454323` bit-for-bit, confirming the pickle is exactly
the run behind the 12.8396 ml figure.

**Balls are not naive, though — one mitigation already exists for ball-rod
overlap.** `scripts/support_placement/joints.py`'s module docstring
(lines ~30-45) documents an "enclosure diet": a certificate-based check
(`_ball_enclosed`, `joints.py:339`) that **drops** a joint ball from the
list *before* it ever reaches `volume_report` if the entire sphere is
provably inside the union of its touching members' own frustums. Per that
docstring, this already dropped **72.5% of joint-ball volume (6.54 of
7.60 ml)** pre-emission. So `ball_ml=1.3495` is already the *survivors*
of a real overlap filter — but it is filtered by "does the ball touch
volume at all," not "how much of the surviving ball's remaining volume
still overlaps." Member-member (rod-rod) overlap at junctions has **zero**
such mitigation: `volume_report`'s member loop (`tree.py:522-531`) has no
analogous check, and structurally can't — every child member's `p0`
coincides with its parent's surface at the junction by construction (that's
what makes it a connected tree), so every parent-child pair of frusta
necessarily overlaps in the neighborhood of their shared joint, and that
shared neighborhood's volume is counted once per member touching it.

---

## 2. Measurement — analytic voxel rasterization of the union

### Method

Script: `/Users/craig/.claude/jobs/0ddf6d41/tmp/vol_audit_voxelize.py`
(re-runnable: `uv run --frozen python vol_audit_voxelize.py --pitch <mm>`
from the worktree, per the run pattern in `run_engine_full.py`).

Loads `engine_run_state.pkl`'s `members` (1625, `dict(p0,p1,r0,r1,kind,owner,embed_end)`)
and `balls` (1102, `dict(center,r,owner)`) — the exact live set behind the
12.8396 ml figure (`n_members`/`n_balls` match the authoritative numbers
exactly, and re-running `tree.volume_report` on them reproduces 12.8396 ml
bit-for-bit, see §1).

For each primitive:
- **Frustum** (flat-capped cone, axis `p0->p1`, radius `r0` at `p0` linearly
  tapering to `r1` at `p1` — the same shape `tree.py`'s formula integrates):
  `u=(p1-p0)/L; t=dot(X-p0,u)/L; perp=|(X-p0)-t*L*u|; inside iff 0<=t<=1 and perp<=r0+(r1-r0)*t`.
- **Sphere**: `|X-center|<=r`.

Streamed in z to bound memory: for each z-slice of thickness `pitch`
(slice-center `z_c`), a vectorized numpy boolean mask selects primitives
whose padded bbox intersects `z_c`. For each active primitive, ONLY its
own local xy voxel-center window (bounded by its padded bbox, not the
global grid) is tested — vectorized point-in-frustum / point-in-sphere —
and:
- **(a) sum-of-parts**: every primitive's own True-cell count is added
  independently (this deliberately double-counts overlaps — it exists
  only to validate the rasterizer against the engine's analytic 12.8396 ml,
  it is NOT a union measurement);
- **(b) union**: the window's mask is OR'd into two persistent 2D boolean
  grids covering the full xy extent — `members_grid` (members only) and
  `full_grid` (members + balls). After all primitives active in a slice
  are processed, each grid's cell count is added to a running total and
  the grid is discarded — only O(Nx·Ny) 2D memory is ever live, never the
  O(Nx·Ny·Nz) 3D volume.

`members-only union` vs `members+balls union` separates ball-overlap from
junction-overlap per the task's request.

### Rasterizer validation (sum-of-parts vs analytic 12.8396 ml)

| pitch (mm) | voxel sum-of-parts member_ml | error vs analytic 11.4901 | voxel sum-of-parts ball_ml | error vs analytic 1.3495 |
|---|---|---|---|---|
| 0.40 | 11.4869 | −0.028% | 1.3469 | −0.197% |
| 0.15 | 11.5131 | +0.201% | 1.3498 | +0.019% |
| 0.10 | 11.4905 | +0.003% | 1.3494 | −0.013% |

All well within "a few %" — the rasterizer is validated against the
engine's own analytic formula at every tested pitch, and error shrinks
toward 0.10 mm as expected.

### Union / overlap results (convergence across pitch)

| pitch (mm) | sum-of-parts total_ml | **members-only union_ml** | **members+balls union_ml** | overlap total_ml | overlap % of sum | members-only (rod-rod) overlap_ml | rod-rod overlap % |
|---|---|---|---|---|---|---|---|
| 0.40 | 12.8338 | 8.4269 | 8.7591 | 4.0747 | 31.75% | 3.0600 | 26.64% |
| 0.15 | 12.8629 | 8.4303 | 8.7545 | 4.1084 | 31.94% | 3.0828 | 26.78% |
| **0.10** | **12.8398** | **8.4108** | **8.7372** | **4.1026** | **31.95%** | **3.0796** | **26.80%** |

Converged (Δ between 0.15 mm and 0.10 mm is <0.3% on every figure).
Best estimate (finest pitch, 0.10 mm): **true union = 8.7372 ml**, i.e.
**4.1026 ml (32.0%) of the reported 12.8396 ml is double-counted overlap.**

Decomposition (identity `overlap_total = overlap_rod-rod + overlap_ball-rod`
checked exactly at every pitch, residual 0.0 ml by construction — see
`identity_check_residual` in each JSON):

- **Rod-rod junction overlap: 3.0796 ml** — 75.1% of total overlap, 26.8%
  of the member-only sum. This is the dominant term and has **no existing
  mitigation** in the code (§1).
- **Ball-rod overlap: 1.0230 ml** — 24.9% of total overlap. Of the 1.3495 ml
  of ball volume that *survived* the enclosure-diet filter (§1), 1.0230 ml
  (75.8%) is still redundant with the members' own union — i.e. even the
  diet-surviving balls are mostly (but not entirely) already covered by
  member geometry; only ~0.326 ml of ball volume is genuinely new solid
  the members' union didn't already have.

Both halves of Craig's hypothesis — "rod-rod at junctions" and "ball-rod
at joints" — are confirmed as real, measured effects; rod-rod is ~3x the
size of ball-rod.

Full JSON output at every pitch:
`/private/tmp/claude-501/-Users-craig-Documents-Claude-Projects-D-D-Campaigns/f0760b14-ba8b-49b8-8c2c-e73e83d65858/scratchpad/audit/voxel_result_pitch0.40.json`
(smoke run, `/tmp/smoke_04.json` originally, numbers reproduced above),
`voxel_result_pitch0.15.json`, `voxel_result_pitch0.10.json` (all three in
this audit directory except the 0.40 smoke run, which was written to
`/tmp/smoke_04.json` during script validation before the two requested
pitches were run into the audit dir).

---

## 3. Reconciling with ChituBox's 9.99 ml

### What the emit added beyond the 12.8396 ml figure

The delivered file
(`/Users/craig/.claude/jobs/0ddf6d41/tmp/treant_v4_engine_run_TMP.chitubox`,
produced by `emit_final.py`, reparse-verified in
`emit_reparse_results.json`: `n_pads_written=53, n_bridges_written=52`,
both match reparsed counts) adds **53 foot pads + 52 MST raft bridges**
that are NOT part of the 1625-member / 1102-ball set behind 12.8396 ml.

**Measured** (not estimated from templates — measured directly from the
actual mesh triangle blobs in the delivered file, via
`/Users/craig/.claude/jobs/0ddf6d41/tmp/vol_audit_pads_bridges.py`, using
the divergence-theorem signed-tetrahedron-sum volume formula on each
record's own closed triangle-soup mesh, `chitubox_inject.Project.blob()`):

- 53 pads: **0.03963 ml** total (0.000748 ml each, essentially uniform —
  consistent with identical small foot-disc geometry cloned from one template)
- 52 bridges: **0.16621 ml** total (0.000817–0.006141 ml each, varying with
  span length, mean 0.003196 ml)
- **Pads + bridges total: 0.20584 ml**

This is a *measurement* of what's physically in the file, not a guess —
but it does **not** attempt to detect overlap between a pad's disc and the
support column's own foot flare directly above it (a real possibility,
unquantified — flagged as an open unknown below, not assumed to be zero).

### Composite reconciliation

| quantity | value (ml) | source |
|---|---|---|
| Engine-reported total (naive sum) | 12.8396 | authoritative, `tree.volume_report` |
| **Measured union** (members+balls) | 8.7372 | this audit, voxel pitch 0.10mm |
| + pads/bridges (measured) | +0.2058 | this audit, mesh-volume measurement |
| **= composite "true print volume" estimate** | **≈ 8.943** | sum of the two measured figures above |
| ChituBox reported | 9.99 | Craig, slicer UI |
| **Residual gap (ChituBox − composite)** | **≈ 1.05 (≈11.7%)** | unexplained by this audit |

**What IS explained by measurement:** the 12.8396 → ~8.94 ml drop (a
3.9 ml, 30% reduction) is fully accounted for by (a) 4.10 ml of measured
geometric overlap in the member+ball set and (b) 0.21 ml of measured
pad/bridge volume the 12.8396 figure never included in the first place
(this pushes the *other* direction — more true resin than 12.8396's
member+ball subset alone — but is small next to the overlap correction).
This closes **most but not all** of the 12.8396-vs-9.99 gap: **2.85 ml
total gap, of which ~1.80 ml (63%) is explained by measured overlap net
of the pad/bridge addition, leaving ~1.05 ml (37%) unexplained.**

**What remains unknown about ChituBox's estimator (evidence-bound, not
speculation):**

1. **Code-level evidence this audit did find**: `chitubox_emit.py:439-447`
   (`synth_member_chain`) builds each member's record blob via
   `parts = [frustum_mesh(...)]` then `parts.append(sphere_mesh(...))` for
   any owned balls, then `blob = np.concatenate(parts)` (line 447) — a
   **flat concatenation of separate triangle soups, not a boolean/CSG
   union**. So the delivered mesh geometry itself is physically
   overlapping/non-manifold at every ball-bearing joint, and separately
   at every member-member junction (each member is its own record per the
   `for mi, m in enumerate(members)` loop at line 415 — there is no
   cross-member merge at all). This means ChituBox received the same
   overlapping geometry our accounting double-counts.
2. Given (1), the most parsimonious explanation for ChituBox's 9.99 sitting
   close to our *measured union* rather than our *naive sum* is that
   ChituBox's displayed resin-volume estimate comes from **slicing the
   actual layer geometry** (rasterized/filled cross-sections per layer),
   which — regardless of how many overlapping shells the source mesh
   contains — inherently yields the true union, the same way our own voxel
   rasterizer does. **This is an inference from the numbers matching,
   not something this audit directly verified inside ChituBox** — we did
   not instrument or reverse-engineer ChituBox's own volume computation,
   only the file we handed it and our own independent voxel measurement.
3. The ~1.05 ml (11.7%) residual is unexplained. Candidate mechanisms this
   audit did **not** measure and is **not** claiming as fact: (a) ChituBox
   may clip/boolean-subtract the portion of embedded contact geometry
   (stub tips, strut anchor feet — both have `embed_end` set) that lies
   *inside* the model mesh, which our voxel rasterizer does not do (it
   voxelizes the support primitives in isolation, with no model-mesh
   clipping); (b) ChituBox's own slicer resolution/quantization; (c)
   possible pad/flare overlap this audit didn't measure (§ above); (d) any
   difference in what ChituBox's UI number actually reports (e.g. whether
   it includes the base raft, exact same delivered file/generation as
   opposed to a possibly-re-sliced or manually-touched-up copy). None of
   these were tested — they are listed as candidates only.

---

## 4. Verdict and proposal

**Verdict: Craig's hypothesis is CONFIRMED. The engine's volume accounting
double-counts overlap, and it is the dominant source of the 12.8396-vs-9.99
gap** (measured overlap 4.10 ml / 32.0% of the reported total; net of the
missing pad/bridge volume, overlap-correction alone explains 63% of the
gap to ChituBox's number). Rod-rod junction overlap (3.08 ml) is ~3x
larger than ball-rod overlap (1.02 ml) — both are real, but if only one
fix were prioritized it should be the member-member junction case, which
has zero existing mitigation (unlike balls, which already have the
enclosure-diet filter).

**Proposal only (no code changes made or suggested as a diff) — how the
engine should report volume honestly:**

- Report the **measured/voxelized union** as the headline "resin volume"
  number, not the sum-of-independent-primitives. The sum-of-parts number
  (today's `member_ml + ball_ml`) is still useful internally (it's what
  `tree.volume_report` validates cleanly against, and it's a fast O(N)
  closed-form computation with no rasterization cost) but should be
  relabeled something like `sum_of_parts_ml` rather than presented as
  "the" volume, since it structurally overstates true resin use by ~30%
  on this model.
- A full-precision CSG boolean union is the "correct" way to get the true
  union analytically, but is expensive/complex for ~2700 primitives; this
  audit's voxel-rasterization approach (validated to <0.3% against the
  analytic sum, converged across three pitches) is a cheap, good-enough
  substitute — at 0.10 mm pitch the whole 1625-member/1102-ball forest
  rasterized in **3.5 seconds**, so it's cheap enough to run as part of
  the regular reporting path, not just an offline audit.
- If keeping the fast analytic sum as the *primary* number for speed
  reasons, at minimum report both numbers side by side (`sum_of_parts_ml`
  and `union_ml`) so nobody mistakes one for the other, plus the pads/
  bridges the emit stage adds (today invisible to the engine's own
  volume report entirely — `tree.volume_report` never sees them, only
  `emit_final.py` does).
- Given the enclosure-diet precedent already in `joints.py` for balls,
  the same kind of engineering effort — a cheap conservative certificate
  that catches the common/easy overlap cases without needing a full
  union — could plausibly be extended to member-member junctions, which
  is where most of the measured overlap (75%) actually lives and which
  currently has no mitigation at all.

---

## Files

- Code cited: `scripts/support_placement/tree.py:515-537`,
  `scripts/support_placement/engine.py:93-114,188-200,258-311`,
  `scripts/support_placement/joints.py` (module docstring ~L30-45, `_ball_enclosed` L339),
  `scripts/support_placement/chitubox_emit.py:338-457` (`_assign_balls_to_members`, `synth_member_chain`)
  — all in the worktree at
  `/private/tmp/claude-501/-Users-craig-Documents-Claude-Projects-D-D-Campaigns/f0760b14-ba8b-49b8-8c2c-e73e83d65858/scratchpad/wt-emit-final/scripts/support_placement/`.
- Measurement scripts (re-runnable at any pitch, verifier can re-run at a
  different pitch than 0.15/0.10 used here):
  - `/Users/craig/.claude/jobs/0ddf6d41/tmp/vol_audit_voxelize.py --pitch <mm>`
  - `/Users/craig/.claude/jobs/0ddf6d41/tmp/vol_audit_pads_bridges.py`
- Raw measurement outputs:
  - `/private/tmp/claude-501/-Users-craig-Documents-Claude-Projects-D-D-Campaigns/f0760b14-ba8b-49b8-8c2c-e73e83d65858/scratchpad/audit/voxel_result_pitch0.15.json`
  - `/private/tmp/claude-501/-Users-craig-Documents-Claude-Projects-D-D-Campaigns/f0760b14-ba8b-49b8-8c2c-e73e83d65858/scratchpad/audit/voxel_result_pitch0.10.json`
  - `/private/tmp/claude-501/-Users-craig-Documents-Claude-Projects-D-D-Campaigns/f0760b14-ba8b-49b8-8c2c-e73e83d65858/scratchpad/audit/pads_bridges_result.json`
