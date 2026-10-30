# Original Round base STLs

Byte-identical copies, made with `cp -p`. Never edit these files. Plinth tests and the clean-clone gate read them from here (dev-loop design section 9).

Source directory: `/Users/craig/code/hughescr/dnd-campaigns/minis-to-process/`

| File | Source path | Size (bytes) | Triangles | SHA-256 |
|---|---|---|---|---|
| `Round 25.stl` | `/Users/craig/code/hughescr/dnd-campaigns/minis-to-process/Round 25.stl` | 149884 | 2996 | `c293b0b6a397cc62a25445a1281ece7dbfb6c6ae07c68d0f3f191e10a35b2b17` |
| `Round 50.stl` | `/Users/craig/code/hughescr/dnd-campaigns/minis-to-process/Round 50.stl` | 300784 | 6014 | `f2de1f789a9ed2bfbf2dafb65b8e3f44dcd391de7c0da3563dde8ba97d13d104` |
| `Round 100.stl` | `/Users/craig/code/hughescr/dnd-campaigns/minis-to-process/Round 100.stl` | 98384 | 1966 | `cb6b4b0687f1e8f02a8fc59dc1391931e615417c42a23a398ad8824c65d04f82` |

Each hash was measured with `shasum -a 256` on both the source and the copy; the two values were identical.

Format: binary STL, header begins `STLB ATF 13.14.0.145`, 50 bytes per triangle.

Drawn pose (measured from the files): the disc axis is Z. The model spans z = -3.5 to 0 mm. The large plain face is at z = -3.5 and points down (-Z): this is the decorated top, drawn face down. The other side (normals +Z) has a raised outer lip and raised rings (one ring on Round 25, three on Round 50, five on Round 100) standing on a recessed floor at z = -2.5; the lip and ring ends are at z = 0. That side is the underside.
