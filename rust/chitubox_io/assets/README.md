# ChituBox opaque assets

`empty.chitubox` is the 420-byte empty ChituBox project used as the
from-scratch template.

`two-supports-preview.bin` is the exact opaque preview span extracted from
bytes `[420, 27416)` of
`scripts/tests/fixtures/chitubox/two-supports.chitubox`. That fixture is an
original ChituBox-UI-authored project supplied on 2026-07-26; it is native
ground truth, not library output. The preview is 26,996 bytes and has SHA-256:

```
e7c0f523b3d56a1aad13a465247a032d3695d38b395df2e5fe1b64bb7061ffde
```

The writer embeds this asset solely as SPEC-CHITUBOX-RS R1's compatibility
fallback when the first model is added to a project whose preview is empty. It
does not interpret, regenerate, or modify the preview; existing nonempty
previews remain verbatim. Runtime code depends only on this committed asset,
never on the test fixture or its path.
