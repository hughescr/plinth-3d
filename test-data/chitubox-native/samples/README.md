# ChituBox v1.4.0 sample files

Craig made these files in ChituBox v1.4.0, starting from the original flat `Round 25.stl`. That source is oriented top-down, and 01–05 keep that orientation.

Purpose: each file differs from the one it builds on by a single change, so comparing them shows which file fields that change touches.

| File | Contents |
| --- | --- |
| `01-round25-plain.chitubox` | Imported and saved, nothing else. |
| `02-round25-lifted.chitubox` | Raised off the plate, no supports. If ChituBox drops the model back to the plate, save it anyway and note that it did. |
| `03-round25-supports.chitubox` | One support of each kind ChituBox offers, with the "no raft" setting (which still makes ChituBox's default mat, note a). |
| `04-round25-supports-raft-connected.chitubox` | 03 with the "connected" raft (note b). |
| `04-round25-supports-raft-hex.chitubox` | 03 with the hex raft (note c). |
| `05-two-plates.chitubox` | Two plates, each holding a supported model with a raft. |
| `06-round25-flipped.chitubox` | 01 flipped top-up, nothing else. Shows how ChituBox stores a rotation. |
| `07-round25-on-edge-braced.chitubox` | Round 25 stood on its edge, so supports are tall enough for ChituBox to generate auto-bracing between them (note e). |

## Notes from Craig

- (a) ChituBox's "no raft" setting still generates a raft: a large plain mat under the supports, which "looks terrible". So 03 contains that default mat.
- (b) 04-connected: each support base is linked to nearby ones.
- (c) 04-hex: one mat with hex-shaped holes under the whole model; the supports land in it.
- (d) He prefers the connected style.
- 02: "Pretty sure it kept it raised."
- 03, around the edge: one each of Heavy, Medium, Light and Ultralight "adaptive" supports; then Ultralight supports of each further kind, in order: "vertical", "upright", "forked branch", "forked joint", "forked small"; then a pair of "forked tree support", which did actually branch. He could not get the other kinds to branch.
- 03, on the inner ring rib: model-to-model supports, one of each type, placed in turn around the ring, showing different articulations and head shapes.
- 03 has no auto-bracing: no support was tall enough to trigger it (see 07).
- (e) 07: several support types, with auto-bracing between them. The support bases land on the raft in a more complex pattern. The longer supports show new kinds of articulation, and he included a range of them. The model is oriented differently from 01–06: stood on its edge.
