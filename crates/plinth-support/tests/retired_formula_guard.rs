//! Source-text regression guard for the retired contact-inset arithmetic.
//!
//! Phase B-2 (owner ruling OD-8) deleted `embed_inset_mm` and both `.max(0.5)`
//! cosine floors from `brace.rs` and replaced them with the derived contact
//! physics in `crate::contact` (PR-1..PR-4).
//!
//! This guard exists because the formula survived a previous deletion attempt.
//! It is deliberately a SOURCE-TEXT check rather than a behavioural one: a
//! behavioural test cannot notice the arithmetic being reintroduced at a fifth
//! call site, and the census showed the disputed paths are inert on today's
//! reachable corpus, so nothing else would fail if it came back.
//!
//! Prose is exempt. The retired names appear in explanatory comments on
//! purpose — the reasoning about WHY the floor was the conservative branch
//! rather than the grazing rejector it was long believed to be is worth
//! keeping, and erasing the names would make that history unsearchable.

use std::path::{Path, PathBuf};

/// Files that must not contain the retired arithmetic in live code.
const GUARDED: [&str; 4] = ["brace.rs", "cco.rs", "rewire.rs", "initial.rs"];

/// The retired arithmetic, by source text.
///
/// `embed_inset_mm` — `contact_depth + (r + standoff)/cos`, which left the
/// depth term undivided and so under-masked by `h*(1/|cos| - 1)`.
/// `.max(0.5)` — the cosine floor that fed it, which made the mask SMALLER and
/// the check STRICTER, and hid the divergence at grazing incidence behind a
/// constant 2.50 mm inset.
/// `config.contact_depth_mm` — **#51's own idiom**, and the reason this guard
/// needed extending. `cco::clearance_rows` and
/// `rewire::validate_changed_geometry` built a FLAT inset pair from it
/// (terminalward) and from `standoff_mm` (rootward) instead of the PR-2 mask.
/// The rootward half of that pair made the model-anchor clearance test
/// unsatisfiable by exactly the arm's own radius — 31,627 of 31,627 feasible
/// candidate arms refused on one Warhorse island — and no behavioural test
/// noticed, because the family simply never won. After the convergence there is
/// NO other use of `config.contact_depth_mm` in either file, so the name is an
/// exact and stable ban: it cannot come back without this guard failing.
///
/// `config.standoff_mm` is deliberately NOT banned. It is the kernel's
/// clearance THRESHOLD argument at both sites and belongs there; the positive
/// guard below is what catches its return as an inset.
const RETIRED: [&str; 2] = ["embed_inset_mm", "max(0.5)"];

/// The flat-inset idiom, banned only where it was the defect.
///
/// `brace.rs` legitimately keeps `contact_depth_mm` on its own `Config` — it is
/// PR-1's `floor_depth_mm`, the seat MINIMUM, which is a different quantity
/// from a clearance inset. The two CCO-side files have no such use left: after
/// the convergence the name appears in neither, so banning it there is exact.
const RETIRED_FLAT_INSET: [(&str, &str); 2] = [
    ("cco.rs", "config.contact_depth_mm"),
    ("rewire.rs", "config.contact_depth_mm"),
];

/// The convergence, by source text: every file that builds a `segment_clearance`
/// inset pair for a contact-declaring member must go through the ONE shared
/// mask.
///
/// This is the positive half of the guard, and it exists because the negative
/// half alone is not enough. Banning a name stops that name coming back; it
/// does not stop a fifth site inventing a NEW flat inset under a new name.
/// Requiring the shared helper does: a site that stops calling it has either
/// deleted its clearance screen or built its own mask, and both are exactly the
/// drift #36 recorded and #51 paid for.
const CONVERGED: [(&str, &str); 3] = [
    ("brace.rs", "contact::placed_mask_inset_mm"),
    ("cco.rs", "placed_mask_inset_mm"),
    ("rewire.rs", "placed_mask_inset_mm"),
];

fn support_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Strip `//`-style comments so prose mentioning the retired names is exempt
/// while live code is not.
///
/// Deliberately conservative: it does not attempt to parse string literals or
/// block comments, so it can only ever over-report (fail on something that is
/// actually inert), never under-report. A guard that fails loudly on a false
/// positive is the right failure direction here.
fn code_only(line: &str) -> &str {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") {
        return "";
    }
    match line.find("//") {
        Some(index) => &line[..index],
        None => line,
    }
}

#[test]
fn the_retired_contact_inset_arithmetic_is_absent_from_live_code() {
    let mut offences = Vec::new();
    for file in GUARDED {
        let path = support_src().join(file);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("guarded source {} must be readable: {error}", file));
        for (number, line) in source.lines().enumerate() {
            let code = code_only(line);
            for needle in RETIRED {
                if code.contains(needle) {
                    offences.push(format!("{file}:{}: {}", number + 1, line.trim()));
                }
            }
        }
    }
    for (file, needle) in RETIRED_FLAT_INSET {
        let path = support_src().join(file);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("guarded source {file} must be readable: {error}"));
        for (number, line) in source.lines().enumerate() {
            if code_only(line).contains(needle) {
                offences.push(format!("{file}:{}: {}", number + 1, line.trim()));
            }
        }
    }
    assert!(
        offences.is_empty(),
        "the retired contact-inset arithmetic is back in live code.\n\
         `embed_inset_mm` and the `.max(0.5)` cosine floor were deleted by \
         Phase B-2 and replaced by the derived contact physics in \
         `crate::contact` (PR-1 seat depth, PR-2 mask extent, PR-3 emergence \
         admissibility). If you need a clearance mask, call \
         `contact::mask_extent_mm`; if you need to reject a grazing contact, \
         that is PR-3, not a cosine floor.\n\
         Offending lines:\n  {}",
        offences.join("\n  ")
    );
}

/// The guard must actually be able to see the thing it guards against —
/// otherwise a refactor that renames the files or changes the comment syntax
/// would silently disarm it.
#[test]
fn the_guard_detects_the_retired_arithmetic_when_it_is_present() {
    assert!(code_only("    let cosine = abs_cos_raw.max(0.5);").contains("max(0.5)"));
    assert!(code_only("    let inset = embed_inset_mm(a, b, c, d);").contains("embed_inset_mm"));
    assert!(
        code_only("        inset1: config.contact_depth_mm,").contains("config.contact_depth_mm")
    );
    // ... and must exempt prose.
    assert!(code_only("    // the retired embed_inset_mm and its max(0.5) floor").is_empty());
    assert!(code_only("/// `embed_inset_mm` with `.max(0.5)`").is_empty());
    // Trailing comments are stripped, leading code is not.
    let mixed = code_only("    let x = 1; // embed_inset_mm mentioned here");
    assert!(!mixed.contains("embed_inset_mm"));
    assert!(mixed.contains("let x = 1;"));
}

/// The other half of the convergence: the shared mask is REACHED from every
/// site that screens a contact-declaring member.
#[test]
fn every_clearance_site_goes_through_the_one_shared_mask() {
    for (file, needle) in CONVERGED {
        let path = support_src().join(file);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("guarded source {file} must be readable: {error}"));
        let reached = source.lines().any(|line| code_only(line).contains(needle));
        assert!(
            reached,
            "{file} no longer calls `{needle}`. Every site that builds a \
             `segment_clearance` inset pair for a contact-declaring member must \
             use the ONE shared PR-2 mask (`contact::placed_mask_inset_mm`). \
             Three sites diverging from it is defect #36; the rootward half of \
             that divergence is #51, which made the model-anchor clearance test \
             unsatisfiable by exactly the arm's own radius. If this site no \
             longer needs a clearance screen, delete the row from CONVERGED and \
             say why."
        );
    }
}

/// The positive guard must be able to fail — a needle that is present in no
/// file at all would make it vacuous.
#[test]
fn the_positive_guard_would_notice_a_site_that_stopped_converging() {
    let absent = code_only("    let inset = a_mask_helper_that_does_not_exist(member);");
    assert!(!absent.contains("placed_mask_inset_mm"));
    // ...and it really does read live code, not prose.
    assert!(code_only("/// calls placed_mask_inset_mm").is_empty());
}

#[test]
fn every_guarded_file_exists() {
    for file in GUARDED {
        let path = support_src().join(file);
        assert!(
            path.is_file(),
            "guarded source {file} is missing; the guard would silently pass"
        );
    }
}
