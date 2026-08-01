# plinth-metrics

`plinth-metrics` is a disk-first scoring adapter. It owns metric policy,
comparison, and reporting, while `geom_kernel` solely owns spatial geometry and
`plinth-mesh` solely owns `Mesh`, topology, and delicacy. Adapters and boundary
conversions are allowed; duplicate algorithms are not. Audit independence comes
from freshly reopened files plus analytic, injected-corruption, and native
ChituBox fixtures.

Run the release delicacy gate against the durable Treant source input with:

```sh
cargo run -p plinth-metrics --release --example treant_delicacy_gate -- \
  '/Users/craig/code/hughescr/dnd-campaigns/Weeping Treant v1.2.stl'
```

The example accepts binary STL only and builds with seed zero and a 20,000
sample ceiling. Its deterministic digest covers the complete exterior mask,
selected original face ids, and every sample face id, point, normal, and final
weight. It also reports result-owned index-visit counts, duration, and
best-effort RSS. The values are evidence, not a wall-clock threshold: hardware
affects timing and RSS.
