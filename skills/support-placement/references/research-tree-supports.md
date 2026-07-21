# Research spike: rigorous construction of branching support trees

2026-07-21, three parallel research agents (fabrication literature; optimization
theory; cross-domain network design), synthesized for the v4 "descending front"
question Craig posed: *can we identify the load points from physics and build
the tree top-down, merging as we descend, as one repeated operation?*

Short answer: yes — this is a well-studied problem family, our exact
combination is unsolved in any single literature, and the theory hands us two
closed-form upgrades (a load-dependent junction-angle law and a junction radius
conservation law) plus one architectural lesson (greedy construction NEEDS a
rewiring pass; the parallel-column waste we just diagnosed is the textbook
local minimum of greedy merging).

## 1. The formal home of our problem

Minimum-material branching support = **single-sink Gilbert–Steiner /
branched optimal transport** (cost = Σ length·(load)^α, α<1 — Gilbert 1967;
Xia 2003; Bernot–Caselles–Morel, *Optimal Transportation Networks*, Springer
LNM 1955), **plus a gradient constraint** (max angle from vertical — exactly
the Melbourne school's *gradient-constrained minimum Steiner networks*,
Brazil/Rubinstein/Thomas et al., built for underground mine declines),
**plus obstacle avoidance** (the VLSI OARSMT literature: spanning-graph +
maze-routing heuristics, hundreds of terminals routinely).

Each of those three literatures solves two of our three constraints; none
solves all three. Concavity of our cost is real and strong: deflection sizing
gives r ∝ load^(1/4) (cost/length ∝ √load), and the min-radius floor +
per-column pad/flare are fixed costs — buy-at-bulk network design theory
(Goel & Post 2012, "One Tree Suffices") proves **greedy aggregation is an
O(1)-approximation for ANY concave cost** — theoretical cover for keeping a
greedy merge core rather than chasing exact optimization (exact topology is
NP-hard; no known exact algorithm above 2D).

## 2. What the theory hands us, ready to implement

- **Junction angle law (replaces fixed 45/60 merge thresholds):** Xia's
  closed-form Y-junction angles for merging loads m_P, m_Q into m_O:
  θ1 = arccos((k2−k1−1)/(2√k1)), θ2 = arccos((k1−k2−1)/(2√k2)),
  θ3 = arccos((1−k1−k2)/(2√(k1·k2))), with k_i = (m_i/m_O)^(2α). The optimal
  merge angle is LOAD-RATIO dependent: asymmetric merges want the light branch
  to swing in at a steep angle while the heavy trunk barely deviates; equal
  loads approach the 120° Steiner limit as α→0. Degenerates to "don't merge"
  when the required angle exceeds the printability cone — i.e., our cone gate
  survives as the feasibility clamp, but the angle inside it becomes
  principled. α should be calibrated from our actual cost-vs-load curve
  (≈0.5 in the deflection regime; the min-radius floor makes small-load
  merging even more favorable than any pure power law).
- **Junction radius law:** our own sizing restated as a conservation law
  (Murray's-law analog): in the deflection regime r ∝ load^(1/4) ⇒
  **r_parent⁴ = Σ r_child⁴** at a merge; in the axial (fuse) regime areas add
  (r² additive). Cheap sanity gate at every junction; the fatter of
  law-vs-floor wins.
- **Junction degree:** planar optima have degree exactly 3 (Cherkashin &
  Petrov 2023); in 3D degree ≥4 can be optimal but rarely — binary merges are
  the safe default, don't force ternary.
- **Gradient-pinned Steiner points:** when the cone constraint binds, the
  optimal junction sits ON the cone boundary (Melbourne "labelled topology"
  results) — matches what our geometry already does implicitly.

## 3. Prior art in fabrication (what slicers/papers actually do)

- **Vaněk et al. "Clever Support" (SGP 2014):** the canonical greedy
  cone-merge: sort overhang points top-down, intersect reach cones, merge at
  nearest cone–cone intersection, terminate on mesh/plate. Geometry only —
  radii from an EMPIRICAL printability table, no load physics (the authors
  name this as their central gap). PrusaSlicer's SLA "branching supports"
  descends directly from this paper.
- **Schmidt & Umetani (Meshmixer, SIGGRAPH 2014):** closest to Craig's
  descending front — priority queue processed top-down: try to terminate,
  else pair/merge with nearest reachable point, else step downward and
  requeue, with look-ahead steering toward merges; force-based relaxation
  tidies the graph. Radii are printability tapers, not load-derived. Names
  "build-time transient stability" as unsolved future work.
- **Cura tree/organic supports (Ghostkeeper 2017; Thomas Rahm 2022-23):**
  the best obstacle machinery in production: per-radius AVOIDANCE VOLUMES
  (model offset by branch radius + clearance) propagated layer-by-layer
  downward; branch positions relax toward each other and get pushed out of
  the avoidance region each layer. Radius = f(distance-from-tip), not load.
  Known failure modes: radius quantization jaggedness; branches that can't
  reach the plate left hanging.
- **Dumas et al. (SIGGRAPH 2014) scaffoldings:** the ONLY prior work that
  optimizes for stability during the whole print (build-time honesty);
  bridges+pillars rather than trees; their shipped version skips model
  collision entirely.
- **Load-aware sizing:** Local-Barycenter tree supports (CAD 2019) is the
  only paper sizing struts per accumulated branch load (static proxy);
  ET-Sup (Kwok 2021) contributes a cheap clear/obstructed/enclosed
  reachability classification of contact points; HCETS (2024) is the newest
  SLA-specific tree paper (evolutionary, paywalled).
- **Net:** the literature is mature on geometry (cones, avoidance fields,
  merge heuristics), immature on physics. Our differentiators — peel-derived
  load accumulation, angle-aware sizing, build-time honesty, hard
  no-intersection + connectivity g