# Adversarial verification of the volume-accounting audit

Verifier: independent adversarial pass over `volume_audit.md`. Method: re-derive
the geometry from scratch, re-run the auditor's script at two pitches it never
used (0.20, 0.12 mm), AND run a **second, independently-written full-3D
voxelizer** (different membership formula, full 3D union in memory instead of
z-streaming) at 0.25 and 0.13 mm, plus an independent geometric bracket on the
pads/bridges mesh volumes.

Verifier scripts (throwaway):
- `/Users/craig/.claude/jobs/0ddf6d41/tmp/verify_indep_voxel.py` (independent full-3D voxelizer + independent analytic re-derivation)
- `/Users/craig/.claude/jobs/0ddf6d41/tmp/verify_pads_bridges.py` (closed-mesh / convex-hull / bbox bracket on pad+bridge volumes)
- auditor's own `vol_audit_voxelize.py` re-run at `--pitch 0.2` and `--pitch 0.12` → `verify_pitch0.2.json`, `verify_pitch0.12.json`

## 1. Point-in-frustum math — re-derived, CORRECT

Auditor's test (`vol_audit_voxelize.py:75-95`): `u=(p1-p0)/L`, `t=dot(X-p0,u)/L`,
`perp=|(X-p0)-t·L·u|`, inside iff `0≤t≤1 ∧ perp≤r0+(r1-r0)·t`.

My independent re-derivation used a *different* algebraic form to avoid inheriting
any bug: `t = dot(a,d)/dot(d,d)` (a=X-p0, d=p1-p0) giving the axial fraction
directly, radial = `|a - t·d|`, `r_t = r0+(r1-r0)·t`. The two forms are
algebraically identical (auditor's `t/L·L·u = t·d`), and both are the correct
flat-capped cone frustum whose volume is exactly `pi/3·L·(r0²+r0·r1+r1²)` — the
same formula `tree.py:528` integrates. Voxel-center (midpoint) sampling is
standard and unbiased here (proven by the sum-of-parts↔analytic match below).
The overlap decomposition identity
`overlap_total = (member_parts−member_union) + (member_union+ball_parts−full_union)`
telescopes to exactly `member_parts+ball_parts−full_union` — algebraically exact,
consistent with the reported `identity_check_residual = 0.0` at every pitch.
**Math verdict: PASS.**

## 2. Independent reproduction across 5 pitches — CONVERGES, no blocker

| source | pitch | union M+B (ml) | overlap total (ml) | rod-rod (ml) | overlap % |
|---|---|---|---|---|---|
| auditor | 0.40 | 8.7591 | 4.0747 | 3.0600 | 31.75 |
| auditor | 0.15 | 8.7545 | 4.1084 | 3.0828 | 31.94 |
| auditor | 0.10 | **8.7372** | **4.1026** | **3.0796** | 31.95 |
| verifier (auditor script) | 0.20 | 8.7378 | 4.1020 | 3.0811 | 31.95 |
| verifier (auditor script) | 0.12 | 8.7368 | 4.1024 | 3.0792 | 31.95 |
| **verifier (independent full-3D)** | 0.25 | 8.7642 | 4.1136 | 3.0887 | 31.94 |
| **verifier (independent full-3D)** | 0.13 | 8.7311 | 4.0983 | 3.0768 | 31.94 |

All seven measurements — including two from a completely separate
implementation — cluster at union ≈ **8.73–8.76 ml**, overlap ≈ **4.07–4.11 ml**,
rod-rod ≈ **3.06–3.09 ml**. The overlap **fraction is rock-stable at 31.9–32.0%**
across a 3× pitch range and two independent codebases. Total spread ≤0.4%, all
pitch-explainable. The independent full-3D voxelizer at 0.13 mm (8.7311 ml) lands
0.07% from the auditor's 0.10 mm headline (8.7372 ml). **No number failed to
reproduce. No blocker.** (Minor non-monotonicity in the auditor's 0.40→0.15→0.10
union series is within voxel-surface-quantization noise and is bracketed on both
sides by the independent runs.)

Nuance (not a defect): the "ball_addition" term also absorbs any ball-ball
overlap, so 1.02 ml is an *upper bound* on strictly ball-rod overlap; it does not
affect the headline that rod-rod (≈3.08 ml) is ~3× larger and dominant.

## 3. Sanity checks — PASS

- **Analytic 12.8396 ml**: my independent re-derivation of `Σ pi/3·L·(r0²+r0r1+r1²)`
  + `Σ 4/3·pi·r³` off the pickle gives `member=11.490088, ball=1.349542,
  total=12.839630` — **bit-for-bit** with `st['volume']`. The pickle *is* the run
  behind 12.8396 ml.
- **Voxel sum-of-parts vs analytic**: within 0.03% at 0.10/0.12/0.20 mm (e.g.
  0.10 mm member error +0.003%). Confirms the rasterizer is unbiased → the union
  numbers it produces are trustworthy.
- **Pads/bridges 0.2058 ml**: reproduced exactly (pads 0.039634 + bridges
  0.166210). My independent geometric brackets: every checked blob is
  **watertight** (all edges shared an even number of times → divergence-theorem
  volume is valid), every record's volume is **< its bbox** (53/53, 52/52), and
  **div-volume = convex-hull volume to 3 decimals** (pads and bridges are convex
  solids, so their volume is exact and unambiguous). Pad mean 0.748 mm³ (~a
  1.5 mm-radius, ~0.1 mm-thick foot disc) and bridge 0.82–6.14 mm³ scaling with
  span are physically sensible. **PASS.**

## 4. Verdict

**Double-counting is PROVEN by measurement, and is the dominant driver of the
12.84→9.99 gap.** Two independent voxel implementations across five pitches agree
that the true union of the 1625 members + 1102 balls is ≈8.74 ml, i.e. ≈4.10 ml
(32.0%) of the reported 12.8396 ml is geometric overlap counted by more than one
primitive; rod-rod junction overlap (≈3.08 ml, 75% of overlap, and with zero
existing mitigation in `tree.py`) is ~3× the ball-rod term. Craig's hypothesis —
both halves — is confirmed. The code review (`tree.py:515-537` sums analytic
frustum + sphere volumes with no CSG/overlap step) is accurate.

**Does (union + pads/bridges) bracket ChituBox's 9.99? NO — it UNDERSHOOTS, and
the audit is right to flag this as unexplained.** Composite = 8.737 + 0.206 =
**8.943 ml**, which is **1.05 ml (11.7%) BELOW** ChituBox's 9.99. This is not a
bracket: ChituBox reports *more* resin than our full support-primitive union plus
pads/bridges, not less. Worse for a naive bracket, if pads/bridges overlap the
member feet (audit flags this as unmeasured) the true combined union is ≤8.94, so
the residual is *at least* 1.05 ml. What IS solidly established: ChituBox's 9.99
sits **between** the naive sum (12.84) and the measured union (8.74), landing far
closer to the union — which is exactly the qualitative signature you'd expect if
ChituBox slices overlapping shells into a true union. But the auditor correctly
labels the "ChituBox slices to union" mechanism as **inference from matching
numbers, not verified inside ChituBox**, and correctly leaves the 1.05 ml residual
(candidates: unclipped embedded contact geometry, raft/base inclusion, slicer
quantization, pad/flare overlap) as genuinely unknown. The audit does **not**
overclaim on this point.

**Bottom line: the audit's headline (double-counting real, measured, dominant) is
CONFIRMED and reproduces independently. The reconciliation to 9.99 is partial
(≈63% of the gap explained), an ≈1.05 ml / 11.7% residual remains open, and the
audit states this honestly. No blockers found.**
