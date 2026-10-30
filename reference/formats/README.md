# formats

Old-project reference. Facts only; verify before relying; vocabulary and policies here are NOT requirements.

ChituBox file-format facts reverse-engineered from native ChituBox files in `/test-data/chitubox-native/`, and the old independent-audit invariants.

- `chitubox-format.md` - record layouts (header, model records, support record types, triangle soup, pad/bridge). Facts come from native files only; pad dimensions are observed, not proven limits.
- `chitubox-io.md` - the old reader/writer crate contract. Format facts are useful; API and test-gate text is old-code specific.
- `audit-invariants.md` - KEEP THE INDEPENDENCE PRINCIPLES ONLY (the checker reads only the emitted file and shares no types with the engine; each rule needs a negative control built by the real writer). Its X=12 and seating-depth values are OBSOLETE.
