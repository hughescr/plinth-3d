# models

- `as-drawn/` - Round_25/50/100 (flat glue face UP; turned 180 deg about X from the provisioned source, hashes in `sources.tsv`), plus `.asdrawn` marker files (empty; "use exactly as drawn", the portable form of the old `plinth-no-rotate` tag) and the two synthetic probes that were marked. `README-as-drawn.md` explains each pose and mentions old cargo tooling that is not carried over.
- `synthetic/` - probes `make_box`, `make_overhang_table`, `make_raking_blade`, `make_thin_fin`, `make_toed_slab` (as provisioned, unturned) and `cube.stl`.
- `corpus-index.tsv` - model names and source digests from the old `corpus/manifest.json`. The old baseline columns (a Python pipeline's results and its goals, such as "fewer cavities is better") were dropped.
