# test-data

Test data for the restart. No code. See `/MANIFEST.tsv` for source path, sha256 and provenance of every file.
- `chitubox-native/` - ONLY files written by ChituBox itself (plus the two source STLs and the provenance README). Regenerated fixtures and plinth outputs were deliberately left out: they are circular (a writer bug reproduces itself) and the hand-built ones all failed in ChituBox.
- `models/` - small test meshes. The 37 real minis stay in `/Users/craig/code/hughescr/dnd-campaigns/minis-to-process` (337 MB); `Weeping Treant v1.2.stl` stays in `/Users/craig/code/hughescr/dnd-campaigns/`.
