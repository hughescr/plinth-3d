# Support-tree construction: literature & cross-domain research spike

Research spike (2026-07-21) prompted by Craig's question: is there a more
rigorous, methodical way to build the support forest than the current
greedy-place-then-iteratively-patch approach? Specifically a "descending
front": identify contact points + per-point load, sweep down from the
model toward the plate, merge branches as reach-cones overlap, size
members from accumulated load, terminate on plate or model.

Answer: **yes, and the descending-front instinct is exactly right.** The
problem has a formal home, other fields solve mathematically identical
problems, and the theory pinpoints why our current greedy-and-patch loop
produces the parallel-column waste Craig is seeing.

## The formal problem

Our task is a **constrained single-sink Gilbert–Steiner / branched
optimal-transport problem**: connect hundreds of load points in 3D down
to a base plane by a branching network, where member cost per unit
length is CONCAVE in carried load (our `r ∝ load^(1/4)` deflection
sizing + a min-radius floor => cost `∝ r² ∝ load^(1/2)` per length, an
economy of scale that rewards merging), members obey a max-angle-from-
vertical cone, and an obstacle (the model) must be avoided.

- Concave-cost branching networks: Gilbert 1967; formalized by Xia
  (2003+) and Bernot–Caselles–Morel (*Optimal Transportation Networks*,
  Springer LNM 1955, 2009). Cost `M_α(G) = Σ w(e)^α · length(e)`, α<1.
  The exponent α is exactly "how strongly does merging pay."
- Gradient (max-slope) constraint: Brazil/Rubinstein/Thomas/Weng/Wormald
  (Melbourne), *Gradient-constrained minimal Steiner trees*, motivated
  by underground mine-decline design. Under a slope cap the Steiner
  geometry becomes a "labelled topology" — junctions sit either at the
  free Fermat point or pinned to the gradient-cone boundary.
- Obstacle avoidance: VLSI OARSMT (obstacle-avoiding rectilinear Steiner
  min-tree). Exact solvers (ObSteiner) and heuristics (FORSTer) handle
  hundreds of terminals + ~100 obstacles in seconds–minutes — our scale.

No single paper solves all three constraints at once. We sit at the
intersection of three literatures that each solve two of the three. That
is not a gap in our understanding — it's genuinely near/past the state of
the art for the specific combination (concave-load-cost + gradient cone +
mesh obstacle + build-time honesty).

Key hardness fact: **finding the optimal TOPOLOGY is NP-hard** (3D
Euclidean Steiner min-tree; the AM-support papers say so outright). For a
FIXED topology, optimal junction POSITIONS are solvable in closed form
(Melzak / Xia recursion). This is why every applied field uses
greedy/incremental construction + local geometric relaxation + occasional
topology mutation — never exact optimization. Our v3.x architecture is in
the mainstream; the issue is we do too little of the "topology mutation"
part, and too late.

## Two closed-form laws we should adopt (low-risk, surgical)

1. **Load-dependent junction ANGLE (replaces the fixed 60°).** Xia's
   Y-junction formula: for parent load `m_O` splitting to children
   `m_P, m_Q`, with `k1=(m_P/m_O)^(2α)`, `k2=(m_Q/m_O)^(2α)`,
   `θ1 = arccos((k2−k1−1)/(2√k1))`, `θ2 = arccos((k1−k2−1)/(2√k2))`,
   and the merge is only worthwhile if the incoming angle is below
   `θ3 = arccos((1−k1−k2)/(2√(k1 k2)))`. The optimal merge angle SHRINKS
   as the two branch loads get more asymmetric and widens toward the
   Steiner 120° as they equalize and α→0. A fixed 60° is provably
   cost-suboptimal at every load ratio except one. Clamp the free-optimal
   angle to our vertical cone when it would violate the printability
   limit (the Melbourne "pinned label" case). Match α to our cost curve
   (~1/4 given r∝load^(1/4), cost∝r²·length∝load^(1/2)·length).

2. **Junction RADIUS conservation (Murray's law, our exponent).** Murray:
   `r_parent^k = Σ r_child^k`; the exponent k is set by the cost-vs-
   radius/flow relation, NOT fixed at 3. With our `r ∝ load^(1/4)` sizing
   and load additivity at a junction (`load_parent = Σ load_child`), the
   equivalent conservation law is `r_parent^4 = Σ r_child^4`. This is
   just our own sizing rule restated as a conservation law — but stating
   it explicitly gives a cheap per-junction sanity check / floor (the
   global joint-ball pass and trunk sizing should already satisfy it;
   where they don't, something upstream mis-sized).

Both reduce gracefully to our current fixed-angle / min-radius-floor
behavior in the symmetric / one-branch-dominates cases, so adopting them
refines junction quality without destabilizing the builder.

## Nearest prior art (read these)

- **Schmidt & Umetani, Branching Support Structures (Meshmixer,
  SIGGRAPH 2014).** Closest published match to "descending front": a
  top-down priority-queue growth loop — pop the highest point, try to
  terminate straight down onto plate/model, else pair/merge with the
  nearest reachable point, else grow a step down and re-queue, with a
  short look-ahead so branches drift toward merges. Force-based node
  relaxation as a tidy-up. Radius is a printability taper, NOT load-
  accumulated. Names build-time transient stability as unsolved future
  work — still unsolved in shipped slicers.
- **Vaněk, Galicia, Beneš, Clever Support (SGP 2014).** Greedy nearest
  cone–cone intersection as the merge (Steiner) point; cone–mesh depth-
  buffer collision. O(n). Sizing is an empirical strut length/angle
  table, not physics. PrusaSlicer's SLA "branching supports" is a direct
  descendant — the only production SLA tree-support code, and it is
  geometry-only, no load model. Authors' own stated #1 limitation:
  absence of load/stress computation.
- **Cura / Cura 5 "organic" & PrusaSlicer organic supports (Thomas
  Rahm).** Per-layer bottom-up: MST connect contacts on each layer, relax
  toward each other (merge into trunks), push out of a per-radius
  AVOIDANCE VOLUME precomputed by offsetting the model and bucketed by
  branch diameter (the "influence area propagated downward" collision
  model). Radius = f(distance climbed from tip), not load. Known failure:
  radius quantization causes jagged branches / momentary clearance loss —
  argues for keeping our CONTINUOUS radius, not bucketing.
- **Local Barycenter tree-support (CAD 2019).** Closest thing in the
  literature to load-accumulating radii: per-branch-layer variable
  diameter for "strength," topology by minimizing accumulated path
  length. Still a static strength proxy, FDM.
- **ET-Sup (Kwok, RPJ 2021).** Classifies every contact as clear /
  obstructed / enclosed (can it reach the plate at all) via a ray
  representation, ~1s. Reusable as a feasibility PRE-PASS before the
  sweep — know up front which contacts can't reach the plate.
- **Dumas/Hergel/Lefebvre, Bridging the Gap (SIGGRAPH 2014).** The one
  source that explicitly optimizes point selection for stability DURING
  the whole print, not just final state — the closest prior art to our
  "build-time honesty." (Their shipped tool skips model collision and
  lets bridges lean on the model — a named instance of tolerating the
  model-adsorbed-strut shortcut we try to avoid.)

## Cross-domain analogs (Craig's instinct: sewers, etc.)

- **Constrained Constructive Optimization (CCO)** — vascular-tree
  synthesis (Schreiner 1993, Karch 1999). THE closest algorithmic match:
  add terminals one at a time, trial-attach each to several nearby
  existing segments by inserting a bifurcation, locally optimize the
  bifurcation point (Fermat solve), resize radii by a Murray-law rule
  back from the terminals, keep the lowest-volume legal candidate, reject
  on obstacle intersection. Same geometry, same concave physics, same
  degree-3 junctions as us. Missing: our angle-from-vertical cone
  (biology doesn't print under gravity) and it avoids obstacles by
  reject-and-retry, not routing-around.
- **Gravity sewer layout** — spanning tree over a fixed manhole graph,
  then slope/diameter sizing; two-phase topology-then-sizing, and layout
  treated as a combinatorial search over many candidate trees. Confirms:
  don't lock topology in one greedy pass. (Lives on a street graph, no
  free 3D routing — the framing transfers, the method doesn't.)
- **Mine declines (DOT)** — DP over discretized directions per level,
  each transition an analytic gradient-constrained Dubins link. Clean way
  to enforce a hard max-angle constraint EXACTLY rather than clamping
  after the fact. Single-path though — the routing half, not the merging
  half.
- **Optimal Channel Networks (Rodriguez-Iturbe & Rinaldo)** — energy
  `Σ discharge^γ · length`, γ≈1/2: the SAME concave family as ours.
  Optimized by local rewiring + simulated annealing (detach a branch,
  reattach to a different downhill neighbor, Metropolis accept). The
  move-set is a generic cheap cleanup for exactly the "parallel near-
  duplicate trunks" local minimum greedy construction produces.
- **Physarum solver** — over-connect a candidate graph, solve as a
  resistor network, reinforce high-flow / decay low-flow edges until
  topology emerges from relaxation instead of being greedily locked.
- **Branching architectural columns / ground-structure method** — our
  problem literally upside down (roof loads → few column bases, angle-
  limited members, concave cost). Form-finding / topology optimization;
  too heavy per-print but a design-time calibration tool for our
  angle/cost laws.

## Why our current builder wastes resin (the theory diagnosis)

Greedy nearest-merge commits to a pairing on local distance and never
revisits it; the true optimum needs global topology search or local
rewiring. Our specific symptom — clusters of parallel near-duplicate
full-height columns — is the textbook local-minimum artifact these
algorithms produce, and the exact thing OCN annealing / CCO re-insertion
are designed to escape. Concretely in our engine: `consolidate()` and
`coalesce_columns()` run ONCE, before the stability loop; the loop's
solo-column retry then founds one independent column per orphaned shoot,
and nothing merges them afterward. It's the twin-column problem
reintroduced downstream of the only merge pass.

## Recommended path

Two tiers, both defensible:

**Tier 1 — surgical, keeps the v3.x architecture (do first):**
1. Re-run `consolidate()` + `coalesce_columns()` AFTER the stability loop
   (and after any solo-retry founds new columns), so every column faces a
   merge pass. Cheap; diagnosis estimates ~23% member-volume back from
   the ≤3mm merges alone.
2. Make the solo retry JOIN-FIRST: try attaching to an existing nearby
   trunk (the logic `plan()` already has) before founding a new column.
3. Add an OCN-style bounded stochastic-rewiring cleanup: pick a
   subtree/column, try detach-and-reattach to a cheaper legal parent,
   accept if strictly cheaper (optional annealed acceptance early).
4. Swap the fixed 60° merge gate for Xia's load-dependent angle; add the
   `r_parent^4 = Σ r_child^4` junction check.

**Tier 2 — the descending-front rebuild (Craig's framing), if Tier 1
isn't enough.** A CCO/Meshmixer-style builder: contacts + per-point load
as terminals; ET-Sup clear/obstructed/enclosed feasibility pre-pass;
top-down priority-queue sweep; at each step attempt terminate-on-plate/
model, else merge with nearest reachable terminal at the Xia-optimal
junction (clamped to the vertical cone), else step down and requeue;
radii accumulate load up the tree (Murray `^4`); Cura-style per-radius
avoidance field for O(1) clearance queries during the sweep; a final
OCN rewiring pass. This keeps our differentiators (load-accumulating
radii, build-time honesty, no-intersection guarantee, connectivity gate)
which NONE of the surveyed sources fully have — the geometry is borrowed,
the physics stays ours.

Verify gates (clearance == [], connectivity == [], minima, load) are
architecture-independent and must survive either tier unchanged — they
are how we keep a rebuild honest.

## Sources
Gilbert 1967 (Bell STJ 46:2209); Xia, *Optimal paths related to
transport problems* (2003) + numerical-simulation PDF (Y-junction angle
formula); Bernot–Caselles–Morel, *Optimal Transportation Networks*
(Springer LNM 1955, 2009); Lippmann–Sanmartín–Hamprecht, *Branched OT
with multiple sources* (NeurIPS 2022, arXiv 2210.07702);
Cherkashin–Petrov, arXiv 2309.04202 (degree-3 planar junctions);
Brazil/Rubinstein/Thomas/Weng/Wormald, *Gradient-constrained minimal
Steiner trees* (J. Global Optim. 21, 2001) + DOT2 decline paper;
Goel–Post, *One Tree Suffices* (Theory of Computing 8, 2012); Vaněk et
al., *Clever Support* (CGF/SGP 2014); Schmidt & Umetani, *Branching
Support Structures* (Autodesk/SIGGRAPH 2014); Ghostkeeper/Rahm CuraEngine
TreeSupport; Zhang et al., *Local Barycenter tree-support* (CAD 2019);
Kwok, *Escaping Tree-Support* (RPJ 2021); Dumas–Hergel–Lefebvre,
*Bridging the Gap* (TOG 2014); Schreiner/Karch CCO; Runions et al. space
colonization (2007); Rodriguez-Iturbe & Rinaldo OCN; Tero et al. Physarum
solver; Stringer et al. support-structure review (JMMP 2018).
