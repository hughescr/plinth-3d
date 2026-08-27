# Clippy pedantic/nursery triage (3,817 warnings)

Method: grouped by normalized message, cross-referenced each hit's line against
that file's `mod tests {` boundary (production code precedes it), read every
"suspicious operators" site, every "manual implementation...overflow" site, and
a stratified sample (~45 sites) of "strict comparison" across production and
test code in plinth-support, plinth-hollow, plinth-decimate, plinth-emit,
plinth-metrics, geom_kernel, chitubox_io.

## Class table

| Class | Count | Crates (top) | Verdict |
|---|---|---|---|
| multiply-add precision (`mul_add`) | 534 | geom_kernel, plinth-support, plinth-hollow | Mechanical noise — micro-precision suggestion, not a bug signal |
| strict f32/f64 comparison (3 variants) | 586 | plinth-support (specialized/perception/cco/transit_measurement), plinth-hollow, plinth-decimate, tests corpora | **Mixed — see below** |
| missing doc `# Returns`/`# Panics`/backticks/paragraph-too-long | ~519 | workspace-wide | Pure doc-style noise |
| casting may-truncate/lose-sign/wrap (5 variants) | ~600 | geom_kernel (127), plinth-hollow, plinth-support/perception+brace, plinth-decimate, chitubox_io | **Mostly mechanical given bounded mesh/mm domains; one systemic debt item** |
| function too many lines | 155 | plinth-support (specialized/perception/runner/cco) | Mechanical; largely long test functions, a few real god-functions |
| `pub(crate)` item inside private module | ~230 | workspace-wide | Pure noise (module-privacy pedantry, not an api-surface leak) |
| items-after-statements / long-literal-separators / redundant closure / etc. | ~250 | workspace-wide | Pure style noise |
| first doc paragraph too long | 45 | workspace-wide | Style noise |
| redundant clone | 28 | plinth-support (cco, coverage_closure, designed_embed, joints, perception, specialized, lib), plinth-pipeline | **All 28 sampled are inside `mod tests` — not hot-path, cheap but low-value** |
| passed by value, not consumed | 36 | plinth-support, plinth-hollow, plinth-decimate, geom_kernel, plinth-pipeline | **All sampled sites are inside `mod tests` — not hot-path** |
| manual impl "which can overflow" (midpoint) | 19 | plinth-support (specialized ×9, brace ×2, joints, rewire), plinth-hollow, plinth-decimate | All `(a+b)/2.0` on mm-scale f64 coordinates; overflow is physically unreachable — mechanical, free cheap fix |
| "suspiciously like a bug" operator sequence | 10 | geom_kernel, plinth-mesh, plinth-decimate, plinth-hollow, plinth-support | **All 10 read — all false positives (determinant/frustum-volume formulas, chained `!=`/`\|\|` boolean logic)** |

## Priority classes — detail

**Strict float comparison (586).** The stated distinction holds up on reading:
- **Safe majority**: (a) `assert_eq!`/`==` in tests pinning deterministic
  algorithm outputs (voxel counts, round-half-to-even, frozen fixtures) —
  e.g. `plinth-hollow/src/lib.rs:4127` `assert_eq!(sphere.cavity_voxels, 16_697)` — 47 of plinth-hollow's 47 hits are inside `mod tests` (boundary at line 3989); (b) config/diagnostic snapshot checks comparing a stored constant to its live registered value verbatim, e.g. `plinth-support/src/transit_measurement.rs:103` `self.a_abs_mm2 == policy.a_abs_mm2().diagnostic().get()`; (c) identity checks on a value copied without arithmetic, e.g. `plinth-support/src/specialized.rs:1642` `contact.position_mm == primary.position_mm`.
- **Unsafe / real risk found**: `crates/plinth-support/src/cco.rs:5582` —
  `member.p1_mm()[2].get() == request.plate_z_mm().get()` — decides whether a
  descent stub already terminates at the plate (skip a parent lookup) by
  exact-equating two independently-derived f64 z-coordinates. A sub-ULP
  rounding difference between the member's endpoint and the plate height
  would silently route into the "has a parent" branch instead, producing a
  wrong pricing/topology decision with no error raised.
  `crates/plinth-emit/src/lib.rs:673-674` `same_point(a,b) { a == b }`, called
  at lines 588/641 on `start`/`end` that both passed through
  `sub_center(finite_point(...), center)` — an arithmetic transform of
  independently-sourced primitive endpoints — to choose between emitting a
  raft **pad** primitive or a raft **bridge** frustum. A near-zero-length
  bridge whose endpoints differ by rounding dust renders as a degenerate
  bridge instead of a pad; narrow but real (numerics class, medium severity).
  `crates/plinth-metrics/src/scar.rs:2028` `self.hi == other.lo` inside
  `RealInterval::overlaps` — boundary-touching decides open/closed adjacency
  of computed scar intervals; worth a second look but not fully traced to a
  concrete failing input in this pass (flag, not confirmed).
- Net: **~3 real risk sites found by sampling** out of 586; the other ~580 are
  either test pins or verbatim-copy/sentinel comparisons.

**"Suspiciously like a bug" (10 of 10 read).** All false positives:
`a*e - b*b` / `dot_aa*dot_bb - dot_ab*dot_ab` (determinant of a 2×2 Gram
matrix — correct), `PI/3*length*(r0*r0+r0*r1+r1*r1)` (frustum-volume formula,
correct, appears twice: `cco.rs:5682`, `rewire.rs:1187`), and chained boolean
`!=`/`||`/`<` expressions clippy's lint pattern-matches loosely
(`repair.rs:170`, `cco.rs:1799`, `decimate/lib.rs:2082`). None is a bug.

**Casting truncate/sign/wrap (~600).** geom_kernel's BVH (`bvh.rs:156-157,
263,322-323`, `queries.rs:1065-1272`) stores face/node indices as `i32` cast
from `usize`, wrapping only above ~2.1B faces/nodes — physically unreachable
for MSLA mini meshes (tens of thousands to low millions of triangles), so
practically inert, but it is an **undocumented 32-bit index-width assumption**
baked into the BVH's `Node.a/b: i32` fields with no assertion at the boundary
— a physics-first-constants-adjacent debt item (an implicit limit that isn't
named or enforced) rather than a live bug. `chitubox_io/src/write.rs:107-117`
is the good pattern: the wrap-risk cast is immediately followed by an explicit
`if new < 0 || new > u32::MAX as i64 { return Err(...) }` range check — already
guarded, pure lint noise there.

**Manual midpoint (19 of 19 read).** All `(a+b)/2.0` on millimetre-scale f64
geometry (contact positions, gap bounds, radii). Overflow is not reachable in
this domain; `f64::midpoint` is also marginally more accurate (avoids
intermediate-sum rounding). Free, cheap, zero-risk fix; not urgent.

**Redundant clone (28) / passed-by-value-not-consumed (36).** Every production
file that clippy flagged carries a `mod tests {` boundary earlier in the file;
checking each flagged line number against that boundary shows **all sampled
hits in both classes fall inside test modules**, not the production hot paths
(support-tree build, coverage repair, CCO solve). These are harmless test-code
cleanups, not the batch-first hot-path violations the project doctrine warns
about — no confirmed hot-path clone/copy defect found in this sample.

**Function too many lines (155) / first doc paragraph too long (45).**
Concentrated in `plinth-support/src/specialized.rs` (16) and `perception.rs`
(15) — both already known 8-14k line modules. Per the rules of evidence, size
alone isn't a finding; not independently re-litigated here (out of scope: no
new concrete cohesion problem read/cited beyond what's already tracked).

## Three worst concrete instances (risk-relevant)

1. `crates/plinth-support/src/cco.rs:5582` — exact `==` between a computed
   member endpoint z and a computed plate height, gating a topology branch:
   `member.p1_mm()[2].get() == request.plate_z_mm().get()`
2. `crates/plinth-emit/src/lib.rs:673-674` (called from 588, 641) — exact `==`
   on arithmetically-derived triangle-mesh endpoints deciding pad-vs-bridge
   emission: `fn same_point(a: [f64;3], b: [f64;3]) -> bool { a == b }`
3. `rust/geom_kernel/src/bvh.rs:156-157` — undocumented 32-bit BVH index width:
   `nodes[node_idx].a = lo as i32; nodes[node_idx].b = count as i32;` with no
   assertion that `count`/`lo` fit in i32.

## Noise classes to allow at the workspace level

`clippy::suboptimal_flops`, `clippy::doc_markdown`, `clippy::missing_errors_doc`,
`clippy::missing_panics_doc`, `clippy::items_after_statements`,
`clippy::unreadable_literal`, `clippy::redundant_closure_for_method_calls`,
`clippy::module_name_repetitions`, `clippy::similar_names`,
`clippy::many_single_char_names`, `clippy::private_interfaces` (pub(crate)-in-
private-module family — actually `clippy::redundant_pub_crate`),
`clippy::doc_lazy_continuation`, `clippy::too_many_lines` (size-only, no
independent signal at workspace scale), `clippy::missing_fields_in_debug`,
`clippy::struct_field_names`.

Proposed `[lints.clippy]` (workspace `Cargo.toml`):

```toml
[workspace.lints.clippy]
pedantic = { level = "warn", priority = -1 }
nursery  = { level = "warn", priority = -1 }
# Real signal: keep strict_comparison, cast_possible_truncation,
# cast_sign_loss, cast_possible_wrap, cast_precision_loss, and
# suspicious_operation_groupings at "warn" (inherited from pedantic/nursery
# above); everything below is style noise this project should not carry.
suboptimal_flops = "allow"
doc_markdown = "allow"
missing_errors_doc = "allow"
missing_panics_doc = "allow"
items_after_statements = "allow"
unreadable_literal = "allow"
redundant_closure_for_method_calls = "allow"
module_name_repetitions = "allow"
similar_names = "allow"
many_single_char_names = "allow"
redundant_pub_crate = "allow"
doc_lazy_continuation = "allow"
too_many_lines = "allow"
missing_fields_in_debug = "allow"
struct_field_names = "allow"
must_use_candidate = "allow"
# Manual-midpoint is real but zero-risk on mm-scale f64; leave at warn as a
# free cleanup queue, don't allow it away.
```

Note: `redundant_clone` and `passed_by_value` are not in the allow list —
they're cheap wins even though this sample found them only in tests; leaving
them at warn keeps future hot-path regressions visible.

## Bottom line

Of 3,817 warnings, the overwhelming majority (~3,150+) are mechanical
style/doc noise safe to allow at the workspace level. Of the geometry-risk
classes actually sampled and read line-by-line, **3 confirmed/plausible real
risk sites** were found (cco.rs:5582 confirmed-plausible correctness risk,
plinth-emit/lib.rs:673 confirmed-plausible numerics risk, geom_kernel/bvh.rs
32-bit-index architecture debt — not a live bug at current mesh sizes), plus
one architectural debt note (BVH i32 index width) and zero confirmed bugs in
the "suspicious operators" and "manual overflow" classes (all 10 + 19 read
and cleared).
