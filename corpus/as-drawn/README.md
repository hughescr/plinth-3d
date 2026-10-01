# corpus/as-drawn: Round bases drawn glue face UP, marked "use as drawn"

These three STLs are the Round bases the owner wants printed **exactly as drawn**:
flat glue face UP, stiffener side down against the supports (owner ruling
2026-09-28; GitHub issue #1, owner spec 2026-10-01). Each `<stem>.stl` has an
empty marker file `<stem>.asdrawn` beside it, so `plinth prepare` uses it as
drawn (identity rotation, still centred and lifted off the plate) and runs no
orientation search on it. The marker is the portable form of the
`plinth-no-rotate` tag: git does not keep extended attributes, so a Finder or
`user.xdg.tags` tag would not survive a clone.

## Why copies, and why here

The provisioned `corpus/models/Round_{25,50,100}.stl` are drawn glue face
**DOWN** (the plain face at the authored z minimum; measured in
`plinth-evidence/slice-sweep-2026-09-02/field-2026-09-28/round50.md`, item 1).
Marking those as drawn would print them upside down. `corpus/models/*.stl` is
also gitignored and provisioned from the owner's model folders, so it is not
this repository's to edit. These copies are turned over once and committed
here, as repository-owned fixture data.

## How they were made (once, 2026-10-01, branch `fix/orient-as-drawn-2026-10-01`)

    cargo run --profile gate --offline -p plinth-mesh --example flip_stl_x180 -- \
        corpus/models/<stem>.stl corpus/as-drawn/<stem>.stl
    : > corpus/as-drawn/<stem>.asdrawn

`flip_stl_x180` rotates 180 degrees about X, `(x, y, z) -> (x, -y, -z)`, by
flipping the sign bit of the `y` and `z` f32 of every facet normal and vertex.
Nothing is re-rounded; the header, triangle count and order, vertex order and
attribute bytes are unchanged. That is bit for bit the rotation the dense search
applies when it commits the flip (its antipode convention: 180 degrees about
+X), so a marked copy is oriented to the same mesh the search produced for the
original (`round_bases_ratchet.rs`,
`an_unmarked_disc_is_still_searched_and_its_flip_is_the_as_drawn_pose`).
Exactly 8 bytes per facet differ from the source.

| stem | facets | source `corpus/models/<stem>.stl` sha256 | this file sha256 |
|---|---|---|---|
| Round_25 | 2,996 | `c293b0b6a397cc62a25445a1281ece7dbfb6c6ae07c68d0f3f191e10a35b2b17` | `c470ac7749ec9fe02c32fed68580e75923107917929e4c5d008769291478cdc7` |
| Round_50 | 6,014 | `f2de1f789a9ed2bfbf2dafb65b8e3f44dcd391de7c0da3563dde8ba97d13d104` | `93f2b0d037681aaad36fd0676964e648a7482c4d6c19fba149661f95befb1e91` |
| Round_100 | 1,966 | `cb6b4b0687f1e8f02a8fc59dc1391931e615417c42a23a398ad8824c65d04f82` | `c399c1f04c86e2617e4cb806288ecb0a13da12d7e3eee832cd0f7f7800dc7554` |

The source hashes are the ones in the field run's `inputs.sha256`.

## What checks them

- `crates/plinth-sweep/tests/it/round_bases_ratchet.rs`,
  `the_committed_round_discs_are_marked_and_drawn_glue_face_up`: the production
  probe finds each marker, and the file, used as drawn, stands flat with the
  plain face (the one that reaches the axis) on top.
- The same file's main ratchet takes its as-drawn reason from these markers.
