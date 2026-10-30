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

## Notes from Craig

- (a) ChituBox's "no raft" setting still generates a raft: a large plain mat under the supports, which "looks terrible". So 03 contains that default mat.
- (b) 04-connected: each support base is linked to nearby ones.
- (c) 04-hex: one mat with hex-shaped holes under the whole model; the supports land in it.
- (d) He prefers the connected style.
- 02 (did ChituBox keep the lift?):
- 03 (where each support kind sits on the model):
