# corpus/as-drawn: test models the owner wants used exactly as drawn

Each `<stem>.stl` here is a test model the owner wants printed **exactly as
drawn**, and each has an empty marker file `<stem>.asdrawn` beside it, so
`plinth prepare` uses it as drawn (identity rotation, still centred and lifted
off the plate) and runs no orientation search on it. The marker is the portable
form of the `plinth-no-rotate` tag: git does not keep extended attributes, so a
Finder or `user.xdg.tags` tag would not survive a clone. Finder tags are for the
owner's own model folders; everything in the repository uses the marker
(owner, 2026-10-02).

| stem | pose the owner asked for | the provisioned source is drawn | so the copy is |
|---|---|---|---|
| Round_25, Round_50, Round_100 | flat glue face UP, stiffener side down against the supports (ruling 2026-09-28; issue #1, spec 2026-10-01) | glue face DOWN (`round50.md` item 1) | turned 180 deg about X |
| make_overhang_table | LEGS DOWN (2026-10-02; the dense search turns it legs up) | legs down | a byte copy |
| make_raking_blade | BLADE UP (verdict 2026-09-03; marked as drawn 2026-10-02) | blade DOWN (the blade hangs under the slab) | turned 180 deg about X |

Every other test model is still oriented by the search (owner, 2026-10-02).

## Why copies, and why here

The provisioned `corpus/models/*.stl` are gitignored and provisioned from the
owner's model folders, so they are not this repository's to edit or to mark,
and a marker beside a source drawn the other way up would print it upside
down. So each model is committed here once, as repository-owned fixture data.

## How they are made: never by hand

The recipe is `sources.tsv` (stem, source, turn, source sha256, result
sha256). One example makes the whole folder from it, and checks it:

    cargo run --profile gate --offline -p plinth-mesh --example as_drawn_corpus            # write
    cargo run --profile gate --offline -p plinth-mesh --example as_drawn_corpus -- --check # verify only

It refuses a source whose sha256 is not the recorded one, and a result whose
sha256 is not the recorded one. Turn `x180` is `plinth_mesh::turn_binary_stl_x180`:
`(x, y, z) -> (x, -y, -z)` by flipping the sign bit of the `y` and `z` f32 of
every facet normal and vertex. Nothing is re-rounded; the header, triangle count
and order, vertex order and attribute bytes are unchanged, and exactly 8 bytes
per facet differ from the source. That is bit for bit the rotation the dense
search applies when it commits the flip (its antipode convention: 180 degrees
about +X), so a marked copy is oriented to the same mesh the search produced for
a flipped original (`round_bases_ratchet.rs`,
`an_unmarked_disc_is_still_searched_and_its_flip_is_the_as_drawn_pose`).
Turn `none` is a byte copy.

History: the Round copies were first made on 2026-10-01 (branch
`fix/orient-as-drawn-2026-10-01`) with the `flip_stl_x180` example, one file at
a time. On 2026-10-02 (branch `fix/as-drawn-inputs-2026-10-02`) the turn moved
into the library, `as_drawn_corpus` and `sources.tsv` were added, the table and
blade rows were added, and the example regenerated the Round copies byte for
byte.

| stem | facets | turn | source `corpus/models/<stem>.stl` sha256 | this file sha256 |
|---|---|---|---|---|
| Round_25 | 2,996 | x180 | `c293b0b6a397cc62a25445a1281ece7dbfb6c6ae07c68d0f3f191e10a35b2b17` | `c470ac7749ec9fe02c32fed68580e75923107917929e4c5d008769291478cdc7` |
| Round_50 | 6,014 | x180 | `f2de1f789a9ed2bfbf2dafb65b8e3f44dcd391de7c0da3563dde8ba97d13d104` | `93f2b0d037681aaad36fd0676964e648a7482c4d6c19fba149661f95befb1e91` |
| Round_100 | 1,966 | x180 | `cb6b4b0687f1e8f02a8fc59dc1391931e615417c42a23a398ad8824c65d04f82` | `c399c1f04c86e2617e4cb806288ecb0a13da12d7e3eee832cd0f7f7800dc7554` |
| make_overhang_table | 60 | none | `a776132a2cc4a43298d9ca646a4265a7ba60ed7d37de7618147ac754dfe8c996` | `a776132a2cc4a43298d9ca646a4265a7ba60ed7d37de7618147ac754dfe8c996` |
| make_raking_blade | 24 | x180 | `0979f5812c26b3bd88f6e36bb792573fbdc9c023656c46544f2bffdecddefb97` | `c6f77ffd8df98d31680ab39439d9412381d661f9f2bca19f2a2dda7d2b67615a` |

The source hashes are the provisioned files' (the same as `corpus/manifest.json`'s
`source_digest` and the field run's frozen `inputs.sha256`). `corpus/manifest.json`
itself is the frozen 2026-08-01 baseline record and is not changed by this folder.

## What checks them

- `crates/plinth-orient/tests/it/as_drawn_corpus.rs`:
  `every_committed_as_drawn_file_is_its_recorded_turn_of_its_source_and_is_marked`
  (the folder's STLs are exactly the recipe's rows; each file has its recorded
  sha256 and an empty marker that the production probe reads; regenerated from
  its provisioned source by the library turn it is byte for byte the committed
  file), and
  `the_overhang_table_is_drawn_legs_down_and_the_raking_blade_blade_up` (used as
  drawn the way production uses a marked model: the table's lowest faces are its
  four feet and its top covers the centre; the blade's lowest face is the whole
  slab and nothing as large is at the top; the provisioned blade is checked to be
  blade DOWN).
- `crates/plinth-mesh/tests/it/stl_roundtrip.rs`,
  `turning_over_about_x_is_exact_keeps_order_and_undoes_itself`.
- `crates/plinth-sweep/tests/it/round_bases_ratchet.rs`,
  `the_committed_round_discs_are_marked_and_drawn_glue_face_up`: the production
  probe finds each Round marker, and the file, used as drawn, stands flat with
  the plain face (the one that reaches the axis) on top. The same file's main
  ratchet takes its as-drawn reason from these markers.
