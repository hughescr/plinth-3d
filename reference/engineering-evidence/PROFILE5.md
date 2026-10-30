# Profile 5: Deinonychus on develop 137bb01, untraced, sampled (2026-10-15)

Binary `combined-2/bin/plinth-137bb01` (sha256 8510d6e6...3d34, verified; provenance: build 2 printed 0 Compiling lines, clean
tree). No code changed. One run: timing lock (solo), unsandboxed, default threads (12), `--orient dense --placement sweep`,
**no `--perf`**. `/usr/bin/sample` at 3 ms in back-to-back 4.2 s windows (gaps 5 ms) for the whole process; stderr timestamped
per line (perl) to place P2 on the windows; thread snapshots every 2 s. Script `profile-run5.sh`; folder `fold5.ts` (all stacks,
all threads, with per-window seconds per sample); queries `q.ts` (split under a frame), `busy.ts` (cores per window), `cat.ts`
(pool categories). Key tables: `fold/summary.txt`.

| | this run (sampled) | combined-2 ref (untraced, unsampled) | factor used |
|---|---|---|---|
| wall | 87.57 s (197.2 user, 30.7 sys, 6.43 GB RSS) | **64.45 s** | |
| production phase (3 poses) | 38.02 s (winner measured in 36.85 s) | 25.95 s (winner 25.14 s) | winner x0.682 |
| P2 (second-contact re-plan) | 40.39 s | **31.34 s** | P2 x0.776 |
| output sha | d73742b2... | d73742b2... (same) | |

The sampler, not counting, is the overhead here: it stops all threads per sample, so the 3-pose production phase (more threads)
slows more (1.47x) than P2 (1.29x). Seconds below are "untraced E" = sampled main-thread wall x the factor of that phase. Parallel
parts are overstated a little by a uniform factor and serial parts understated; read every figure as +-20%. Anchors are at
**137bb01** (`git show 137bb01:<path>`): the main checkout has since been switched to `perf/cx-2026-10-15` by another lane, and its
line numbers differ.

## (1) P2 broken down by sub-stage (31.3 s untraced)

P2 runs on one thread (a rayon worker). Busy cores during P2: 1.0-2.4, mean 1.4; the P2 thread is busy 0.91-1.00 of every window.
Other threads add 22 thread-s sampled over windows 12-20, of it 15.8 the clearance kernel (lip-seat read-ahead batches), 0.9
footprint, 1.6 rayon steal/spin.

| P2 part (order of the sweep) | sampled s | **untraced E s** | of which |
|---|---|---|---|
| layer cache rebuild (`LayerCache::build`, lib.rs:2770) + initial capped down sweep | 1.2 | 0.9 | |
| **removal** (`refuse_unrooted_demand`, 2 calls, 2 restarts) + its 2 re-sizings | 8.9 | **6.9** | replay 6.2: `recompute_potential_layer` 5.2 (`any_angle_relax` 3.5, sparse core 1.4), self 0.6, `SurfaceCreep::load_layer` 0.3. All serial (0 idle). |
| unmerge / congestion / stranded / forced merges (sizing, `withdraw_idle_flank_arms`, remedies) | 1.9 | 1.5 | |
| **lip-seat family** (A, head join, B, clearance re-route; `LipSeatRun::run` + `size`) | 19.3 | **15.0** | A ~6.6, B ~5.0, re-route ~3.2 (by window) |
| - `lip_seat::apply` (6 calls) | 15.24 | 11.8 | clearance reads 3.6 (of it 1.0 waiting on the pool), `propose` (path tests) 2.8, `tip_shaft_seat_bites` 2.7, `trial_arm` 0.5, `contact_growth` (memo hits) 0.3, sorts/allocs/self/other 1.9 |
| - re-size after moves (`LipSeatRun::size`, 9 down sweeps incl. 2 undo) | 3.95 | 3.1 | |
| ties: brace lane + model ties (`apply_ties_with_model`) + their sizing | 4.3 | 3.3 | `plan_thicken` 1.1, `new_plane_holds` 0.6, `plan_band_set` 0.5 |
| graze, head-join apply, other | 0.5 | 0.4 | |
| **finish** | 4.24 | **3.3** | `footprint::apply` 1.9, `rigid_share::diagnose` 0.8, `resize_heads_certified` (`joint_size::certify`) 0.5 |
| total | 40.46 | 31.4 | (measured 31.34) |

## (2) What P2 recomputes that P1 already computed with identical inputs

The capped first down sweep changes the sized field (593 arms vs 480): loads, so radii, change along every carrier chain under a
capped tip. Any per-arm read keyed on the sized solid therefore misses P1's answer for most arms. That is why the lsr lane's
process-wide `tip_shaft_seat_bites` memo (keyed on all arm bits + lander height) measured about 0, and why only `contact_growth`
(tip-local geometry, landed in cg: 75% re-asked) paid. What is left, with the key it needs:

| item | P2 untraced cost | key that makes reuse exact | hit rate | E |
|---|---|---|---|---|
| replay baseline carry per layer (P1's call cache kept in the seed) | baseline 1.8 + restart 0.4 | (layer-cache key, births, policy reach params) + a CHAINED hash over layers 0..L of each layer's root list: `(cell, every field BirthRoots::of reads)` of Arm-kind struts with `k_top == L` (demand_replay.rs:706-709). The carry at L is a function of exactly that. | unmeasured. Roots first differ at or below the lowest capped short tip (50 of them over ~386 layers), so the shared prefix is probably short | 0.2-0.6 |
| per-tip footprint reads (`ground_windows`, `graded`, slid normals) | 1.9 | tip seat + seat-zone geometry + the model; but moves re-check neighbours, so only the pure per-tip reads | cg's 75% suggests tip-local questions repeat | <= 0.5; needs a design read |
| head certification (`joint_size::certify`) | 0.5 | every input of the head (post axis, ledger, profiles) | unmeasured; loads differ under capped tips | <= 0.3 |
| layer cache: P2 rebuilds it (lib.rs:2770) | 0.1 | grid + slices (already its key) | 100% | 0.1; costs P1's cache memory for the whole of P2. Drop. |
| `tip_shaft_seat_bites`, clearance reads, path tests | | full sized solid | low (lsr measured) | drop as P1->P2 memo |

The larger recomputation is INSIDE each sweep, with identical inputs, and it is not P1->P2:

- **Read-ahead re-proposes every path-refused candidate in every window that covers it.** `read_movers` (lip_seat.rs:1404-1444)
  proposes candidates `at+1 .. at+window` (window doubles to `READ_AHEAD_MAX` 64, :994) and skips only those already in `ahead`;
  a candidate whose `propose` returned `None` is never stored, so the next round's window (which overlaps the last by ~60
  positions) proposes it again. The winner's own report: 599 k candidates refused by the path test vs ~410 k that passed it (A+B),
  i.e. ~60% of tested candidates are `None`. With overlapping 64-wide windows a `None` candidate can be proposed ~20-30 times.
- **The round loop proposes again what the read-ahead proposed.** lip_seat.rs:1739-1762 calls `propose` for every candidate even
  when `ahead` holds its path (98% of round proposals were read ahead: profile4 counters 517.7 k of 528.9 k), and `trial_arm`
  again for the proposal. Key: (job, candidate index, field generation), the generation bumped at every `struts`/`index`
  mutation inside `apply` (commits, put-backs).

## (3) P2 work that can run on the idle cores without changing results

About 10 cores are idle for all of P2. What can use them, exactly:

- **Seat bites and follower reads in the read-ahead batch (rank 2 below).** Per round, `apply` measures the seat bites of the
  clearing proposals' members (lip_seat.rs:1975-1990) and reads the followers' clearance (:1946-1972) in batches of 1-3 arms on
  the P2 thread (kernel inline: ~264 k follower batches, 238 k of <= 2 rows). Both are pure functions of the trial solids, which
  the read-ahead already knows (it discards the children `propose` returns). Compute them per read-ahead row on the pool, in chunks.
- **Not the per-candidate path tests as tiny tasks** (L4: 9 M tasks, 1.9x slower). Chunks only.
- **Speculative evaluation of the next rounds** (the general form): > 99% of lip-seat rounds commit nothing (each arm commits at
  most once a pass; hundreds of moves against ~0.5 M rounds), so the next rounds' verdicts are a pure function of a frozen field.
  Rank 1 + 2 take the pure reads off the thread; what stays serial is the round's decision code (~1.9 s P2 untraced). Full
  speculative rounds would need `apply`'s round body split into an evaluate/commit pair (ordered counters, ordered pushes to
  `kept_for_slenderness_ids` etc.). Not worth it until 1 + 2 are measured.
- **Removal attempt 2 beside attempt 1** (lib.rs:2906-2960). Attempt 2 (span proxy) starts from `before_struts`/`before_outcome`
  clones, the same state as attempt 1, and writes only accumulators; it runs only when attempt 1 removed something and its
  sizing rolled back. Running it speculatively on a spare core is exact (separate accumulators merged in order). It pays only if
  P2's two replay calls are one pass's two attempts; the profile cannot tell (profile4: 2 calls, 2 removal sizings in P2; the
  winner makes 1 call and removes nothing). Count first.
- **Rigid-share reading and emission beside P2: already done on `perf/ov-2026-10-15`** (first plan's reading and whole emission run
  beside P2; P2's reading deferred until kept). Not counted again here.
- The replay walk itself (layer L needs L-1's carry) and the tie rounds (state carried round to round) have no exact coarse
  parallel form that I found; the rr lane's baseline-ahead overlap left P2 unchanged.

## (4) The winner sweep (25.1 s untraced, on 1 of the ~3-4.6 busy cores)

Winner thread, untraced E (x0.682): lip-seat family **12.6** (`apply` 11.2: clearance 3.2, `propose` 2.8, seat bites 2.6,
`contact_growth` (memo fills) 0.7, `trial_arm` 0.3, other 1.6; re-size 1.4), removal replay 2.1 (1 call, removes nothing), ties
1.7, down sweeps 1.5, finish 3.3 (footprint 1.3, `resize_heads_certified` 1.0, `rigid_share::diagnose` 1.0 -> perf/ov), prefix
(up sweep, reach, setup) ~2.0. Busy cores in production 3.0-4.6: 7-9 idle. The same lip-seat targets apply, with less benefit:
the three poses share the pool, so the winner's pool chunks compete with the losers' until the certified stop.

## Ranked targets (Deinonychus, untraced seconds; prize = what the code costs now, E = expected saving after discount)

Discount: E is about half the ideal saving. These are from an untraced profile (no 2.7x counting inflation), but the speed-ups
assumed (pool scaling, hit rates) are not measured.

| # | target | ceiling (P2 + winner) | **E** | ID/OC | anchors (137bb01) |
|---|---|---|---|---|---|
| 1 | **Lip-seat read-ahead watermark, then round reuse.** (a) Per job, remember the furthest candidate any read-ahead proposed and start the next window after it, so a `None` is proposed once per window sweep, not once per overlapping window. Exact: the read-ahead is a prefetch ("a batch size only", :991-993); the round loop's `propose` stays authoritative, and a candidate that turned `Some` after a commit is still read as the round's own row. (b) Then let the round loop take `(path, children, trial arm)` from `ahead` when the field generation is unchanged. | propose + trial_arm: 3.3 + 3.1 = 6.4 | **1.5-3** (P2 0.8-1.5, W 0.7-1.5) | ID | `lip_seat.rs` 1404-1444 (`read_movers` loop), 1739-1762 (round `propose`), 984 (`ahead`), 2766 (`propose`) |
| 2 | **Widened read-ahead: children's trial arms, follower clearance and seat bites computed per read-ahead row on the pool** (bites in chunks of >= 16 arms through lsr's chunked entry point; followers in the same kernel batch as the movers, which also removes ~264 k tiny dispatches). Two exactness conditions: (i) `tip_shaft_seat_bites` depends on its call's batch through `shallowest_landers` (lower.rs:4108-4175): a per-group (mover + children) result equals today's round call only when no arm of another clearing proposal in that round lands on the group's host; check that at consumption (a few arms) and fall back to today's call; (ii) follower bounds come from standing reads of the children: tag with the field generation. | bites 2.7 + 2.6, followers ~0.9 + ~0.9 = 7.1 | **2-4** (P2 1.2-2.2, W 0.8-1.8) | ID | `lip_seat.rs` 1357-1487, 1940-1990; `plinth-support/src/lower.rs` 3830-3860, 4108 |
| 3 | **Removal: speculative attempt 2 beside attempt 1**, conditional on P2's calls being one pass's two attempts (count first: pass, attempt, removed, rolled back per call). Then, only if a counter shows a long shared root prefix, P1->P2 replay baseline reuse with the chained root key of (2). | replay 6.2 (P2) | **0-2.5** (attempts), 0.2-0.6 (baseline) | ID | `lib.rs` 2873-2970; `demand_replay.rs` 694-760, 761-1070 |
| 4 | Tie rounds (`plan_thicken`, `new_plane_holds`, `plan_band_set`): not assessed for an exact parallel form; rounds carry `TieState`. Read before building. | 2.9 + 1.7 | unknown | ? | `slenderness.rs` 2069 (`apply_ties_with_model`) |
| 5 | Per-tip footprint reads memoised P1->P2 (key: tip seat + seat zone + model). | 1.9 (P2) | <= 0.5 | ID | `footprint.rs` (`apply`, `ground_windows`, `graded`) |
| 6 | `joint_size::certify` memo P1->P2. | 0.5 + 1.0 | <= 0.3 | ID | `lib.rs` 4110, 4839 |

Covered elsewhere (do not double count): `perf/ov-2026-10-15` (rigid-share reading of both plans and the emission off the critical
path: P2 0.8 + winner 1.0 + emit ~0.6, at most ~2.4 s, not yet timed on 137bb01); `perf/cx-2026-10-15` (loser stop latency).

Dropped by measurement or evidence: P1->P2 seat-bite memo (lsr: ~0; calls are 1-3 arms a round, below its 32-arm chunk path, and
the key includes radii the capped sizing changes); clearance-kernel row split (kq: 0); parallel per-candidate path walks as tiny
tasks (L4: 1.9x slower); keeping P1's layer cache for P2 (0.1 s); `PLINTH_THREADS` read per kernel batch
(`geom_kernel/src/par.rs:74`: 0.31 s sampled over the whole run, 0.06 in P2).

**Arithmetic (ID only):** 64.45 - ov (1.5-2.4, untimed here) - #1 (1.5-3) - #2 (2-4) = **about 55-59.5 s**. Under 60 s needs #1 and
#2 at their low ends plus ov; #3 is upside only if its counter shows the attempts pattern. #1 alone is small (one function, one
A/B) and should go first: it also tells how much of `propose` is the repeated `None`s (the E range is wide because a `None` may fail
fast and cost less than a passing path).

## Lanes (file ownership)

- **Lane A = #1 then #2**, one lane because both rewrite `read_movers` and the round loop: `crates/plinth-sweep/src/lip_seat.rs`,
  plus a grouped/chunked bites entry point in `crates/plinth-support/src/lower.rs` (borrow the chunking from `perf/lsr-2026-10-15`,
  not its memo). #1 lands and is timed alone first. Accept on the same sha256 for Deinonychus, BCO, Cylinder plus test-fast; add
  a shadow switch (like `PLINTH_SWEEP_VERIFY_*`) that recomputes every reused propose/bite/follower read the old way and asserts
  bit equality; sabotage it (drop the lander check, drop the generation bump) and see it fail. Check the production wall too: pool
  chunks from three poses compete.
- **Lane C = #3**: `demand_replay.rs` and the removal block of `lib.rs` (2873-2970). Counter run first (shared lock, one Deinonychus
  run with a per-call (pass, attempt, removed, rolled_back) line). `perf/ov` also edits `lib.rs` (finish region, 3961-4300):
  different region, merge carefully.
- **Lane E = #4**, read-only investigation of `slenderness.rs` before any build.
- #5 and #6 only after A and C report.

## Files

`profile-run5.sh`, `runs/deino/` (log with per-line timestamps, `.time`, `.sha`, output), `raw/deino/` (21 sample windows,
`sampler.txt`, `threads.log`), `fold5.ts`, `q.ts`, `busy.ts`, `cat.ts`, `fold/deino.tsv` (all stacks), `fold/p2-main.tsv` (P2
thread), `fold/win-main.tsv` (winner thread, production windows), `fold/prod3.tsv`, `fold/p2-others-full.tsv`, `fold/summary.txt`.
