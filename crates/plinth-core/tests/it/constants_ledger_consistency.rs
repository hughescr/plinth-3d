//! FU-24: the ledger and the registry must name the same constants.
//!
//! `docs/engineering-journal/m0-constants-source-ledger.json` is hand-maintained
//! prose provenance for every constant the live registry
//! (`M0PlaceholderConstants` + `M0EvidenceConstants`, assembled by
//! [`m0_constants`]) actually publishes. Nothing mechanically keeps the two
//! in sync, so they drift silently whenever a constant is added to the
//! registry and the ledger entry is forgotten (or vice versa). This is that
//! mechanical check: parse the ledger's `registry_entries` array, collect its
//! `id` field, and assert the resulting set is exactly the registry's own
//! published id set, in both directions.
//!
//! Kept intentionally cheap: this reads one already-checked-in JSON file and
//! diffs two id sets against the one registry construction every other
//! constants test in this crate already pays for. It does not validate
//! ledger prose content, classification values, or source locators.
//!
//! Recorded future tightening: the `BTreeSet` comparison also passes silently
//! on duplicate `id`s within `registry_entries`, and the ledger's `retired`,
//! `excluded`, and `aliases` arrays are not cross-checked against
//! `registry_entries` at all.

use plinth_core::m0_constants;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct LedgerEntry {
    id: String,
}

#[derive(Deserialize)]
struct Ledger {
    registry_entries: Vec<LedgerEntry>,
}

fn ledger_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("docs/engineering-journal/m0-constants-source-ledger.json")
}

#[test]
fn ledger_registry_entries_name_exactly_the_published_registry_ids() {
    let path = ledger_path();
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "required constants source ledger is unreadable at {}: {error}",
            path.display()
        )
    });
    let ledger: Ledger = serde_json::from_str(&raw).unwrap_or_else(|error| {
        panic!("constants source ledger at {} is not valid JSON matching the expected registry_entries shape: {error}", path.display())
    });

    let ledger_ids: BTreeSet<String> = ledger.registry_entries.into_iter().map(|e| e.id).collect();

    let registry = m0_constants()
        .expect("M0 constants registry must construct")
        .registry();
    let registry_ids: BTreeSet<String> = registry
        .iter()
        .map(|descriptor| descriptor.id().as_str().to_string())
        .collect();

    let missing_from_ledger: Vec<&String> = registry_ids.difference(&ledger_ids).collect();
    let missing_from_registry: Vec<&String> = ledger_ids.difference(&registry_ids).collect();

    assert!(
        missing_from_ledger.is_empty() && missing_from_registry.is_empty(),
        "ledger/registry membership has drifted.\n\
         In the registry but missing a ledger entry ({} ids): {missing_from_ledger:?}\n\
         In the ledger but not published by the registry ({} ids): {missing_from_registry:?}\n\
         Add or remove entries in {} so registry_entries names exactly the registry's own id set.",
        missing_from_ledger.len(),
        missing_from_registry.len(),
        path.display(),
    );
}
