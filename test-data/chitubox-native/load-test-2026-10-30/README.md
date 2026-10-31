# load-test-2026-10-30

Two files from the owner's ChituBox v1.4.0 load test of 2026-10-30. They are one pair: the first was written by plinth, the second is ChituBox's re-save of it. Format findings: `docs/format/chitubox-v1.4.0.md`, section 18.

| File | Written by | Bytes | SHA-256 |
|---|---|---|---|
| `Round 50.keepup.chitubox` | **plinth's writer** (NOT native ground truth) | 218,132 | `d6ef89c07cd98007d1c91c916f305eabcddede7151f8c3a6c01337d685efbb83` |
| `Round 50 resaved.keepup.chitubox` | **ChituBox v1.4.0** (native) | 309,740 | `257578c0ae822e4bfea2bb2b322b94acf9a06a915a97ebf024db52fa2b67a4d6` |

## Provenance

- `Round 50.keepup.chitubox` is a model-only "transport" file made by plinth: the approved 65-degree-tilted Round 50, lifted 5 mm, no supports. Its model name is `Round 50.keepup [transport, not a print candidate]`. It is one of six plinth-written Round files the owner opened in ChituBox v1.4.0 on 2026-10-30. All six opened and sliced.
- `Round 50 resaved.keepup.chitubox` is what ChituBox v1.4.0 wrote when the owner opened the file above and saved it again on 2026-10-30. It is the only ChituBox-written file in this folder.

## Label: the plinth-written file is not native

Plinth wrote `Round 50.keepup.chitubox`, so it proves nothing about what ChituBox writes. It is kept here only to be compared with its re-save.

- The native-file tests leave it out (`PLINTH_WRITTEN` in `crates/chitubox/tests/it/common.rs` and in `crates/acceptance/tests/it/readers_agree.rs`).
- The native-file tests include the re-save: it must parse and rewrite byte for byte.
- `load_test_resave_changed_only_the_preview` (in `crates/chitubox/tests/it/native.rs`) reads both files to check the comparison in the format document.
- Do not copy the plinth-written file into the native set, and do not use it as a fixture for what ChituBox accepts or writes.
