# plinth-metrics

`plinth-metrics` is an independent audit crate.  Its scar delicacy field owns
its triangle BVH, exact-welded vertex radius grid, and point KD trees; it does
not link producer geometry or mesh crates.

Run the release delicacy gate against the durable Treant source input with:

```sh
cargo run -p plinth-metrics --release --example treant_delicacy_gate -- \
  '/Users/craig/code/hughescr/dnd-campaigns/Weeping Treant v1.2.stl'
```

The example accepts binary STL only. It reports deterministic field digest and
result-owned index-visit counts, duration, and best-effort RSS. The values are
evidence, not a wall-clock threshold: hardware affects timing and RSS.
