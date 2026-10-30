# plinth-3d

The restart of plinth, the resin-print support generator for Craig's D&D minis. This repo holds no code yet.

Read in this order:

1. `docs/PRD.md` - the requirements, and the source of truth. It also lives as a Claude doc: https://claude.ai/artifact/X6Ugq2adZ4Vdwu8o7VmEQy
2. `docs/LESSONS.md` - what went wrong in the first attempt and what is known. Input documents and the challenge review: `docs/lessons/`.
3. `docs/interview/` - the owner interview, rounds 1-4.
4. `reference/` - old knowledge documents, copied verbatim. Facts only: verify before relying. Their vocabulary and policies are not requirements.
5. `test-data/` - `chitubox-native/` holds only files written by ChituBox itself; `models/` holds small test meshes. The 37 real minis stay in `../minis-to-process`.
6. `MANIFEST.tsv` - one row per carried-over file: destination path, source path, sha256, bytes, provenance note.
