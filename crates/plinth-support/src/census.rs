//! THROWAWAY INSTRUMENTATION -- embed-incidence census.
//!
//! QUARANTINED behind the `census` cargo feature (MAJOR-8). This whole module,
//! and every call site that feeds it, is `#[cfg]`-gated, so an ordinary build --
//! including `cargo test --workspace` -- contains none of it. Build it with
//! `--features census`; the census example declares that feature as required.
//!
//! Passive, result-inert counters for the 2026-08-04 embed-incidence census.
//! Nothing here is read by production code: every entry point only appends to a
//! process-global buffer, and every buffer is drained by the census example
//! binary. Recording is additionally gated at RUNTIME on `PLINTH_CENSUS=1`, so
//! an un-instrumented baseline can be produced from the same instrumented
//! build.

use std::sync::{Mutex, OnceLock};

use crate::forest::{ContactEnd, MemberKind};
use crate::runner::SupportVerificationSnapshot;

/// Legal model-rung length band, quoted from `brace.rs` config for reporting.
const RUNG_BAND_MM: (f64, f64) = (1.0, 5.0);
/// The floored-cos model-rung inset: 0.3 + (0.5 + 0.6) / 0.5.
const FLOORED_RUNG_INSET_MM: f64 = 2.5;

#[must_use]
pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("PLINTH_CENSUS").as_deref() == Ok("1"))
}

/// One brace model-rung candidate row at the incidence computation
/// (`brace.rs` model-rung construction).
#[derive(Clone, Copy, Debug)]
pub struct RungRow {
    pub abs_cos_raw: f64,
    pub cos_used: f64,
    pub length_mm: f64,
    pub radius_mm: f64,
    pub inset1_mm: f64,
}

/// One member-level embed inset row at the exact-clearance incidence
/// computation (`brace.rs` exact member clearance).
#[derive(Clone, Debug)]
pub struct EmbedRow {
    pub abs_cos_raw: f64,
    pub cos_used: f64,
    pub radius_mm: f64,
    pub inset_mm: f64,
    pub segment_len_mm: f64,
    pub kind: String,
    pub end: &'static str,
}

/// One `cco::apply_model_anchor` strut construction, recorded raw. The normal
/// at the model seat is resolved later in one batched projection.
#[derive(Clone, Copy, Debug)]
pub struct AnchorRow {
    pub stub_end: [f64; 3],
    pub seat: [f64; 3],
}

/// One row handed to `NativeSurface::segment_clearance`, with the
/// `seg_clear_one` INFINITY predicate evaluated locally (identical arithmetic,
/// no kernel change).
#[derive(Clone, Copy, Debug)]
pub struct SegRow {
    pub site: &'static str,
    pub segment_len_mm: f64,
    pub inset0_mm: f64,
    pub inset1_mm: f64,
    /// 0 = no cosine-derived embed inset on this row, 1 = inset built from a
    /// floored cosine (raw |cos| < 0.5), 2 = inset built from raw |cos| >= 0.5.
    pub cos_class: u8,
    /// `true` when this row carries any embed inset at all.
    pub has_embed_inset: bool,
    pub degenerate: bool,
    pub over_masked: bool,
}

/// One embedded contact stub as seeded in `initial.rs`.
#[derive(Clone, Copy, Debug)]
pub struct ContactRow {
    pub position: [f64; 3],
    pub normal: [f64; 3],
    pub tip_radius_mm: f64,
}

/// The disposition of one brace model-rung candidate after the clearance
/// batch and revalidation.
#[derive(Clone, Copy, Debug)]
pub struct RungOutcome {
    /// The cosine the inset was actually built from, recovered exactly from
    /// `inset1`. A value at the 0.5 floor means the raw incidence was grazing.
    pub cos_used: f64,
    pub length_mm: f64,
    pub inset1_mm: f64,
    pub clearance_mm: f64,
    pub over_masked: bool,
    pub viable: bool,
}

/// One PR-8 attestation verdict, for the T7 table.
pub struct AttestationRow {
    pub terminal: u32,
    pub centreline: bool,
    pub rim_engaged: usize,
    pub rim_total: usize,
    pub attested_radius_mm: f64,
    pub refused: bool,
    pub area_deficit: bool,
}

/// One F3 seat-attestation verdict for a TOUCHING family (brace model rung or
/// CCO model anchor), for the T8 table.
///
/// Kept separate from [`AttestationRow`] because the row identity is different:
/// a stub is identified by its terminal, a candidate seat only by the family
/// that proposed it and its position in that family's batch.
pub struct SeatAttestationRow {
    pub family: &'static str,
    pub centreline: bool,
    pub rim_engaged: usize,
    pub rim_total: usize,
    pub attested_radius_mm: f64,
    pub refused: bool,
    pub area_deficit: bool,
}

#[derive(Default)]
pub struct CensusData {
    pub rung_outcomes: Vec<RungOutcome>,
    pub rungs: Vec<RungRow>,
    pub embeds: Vec<EmbedRow>,
    pub anchors: Vec<AnchorRow>,
    pub segs: Vec<SegRow>,
    pub contacts: Vec<ContactRow>,
    pub attestations: Vec<AttestationRow>,
    pub seat_attestations: Vec<SeatAttestationRow>,
    pub appends: std::collections::BTreeMap<&'static str, usize>,
    pub stage_errors: Vec<String>,
    pub scaled_caps: Vec<ScaledCapRow>,
    pub gate_proposals: GateProposalCensus,
    pub retained_load: RetainedLoadCensus,
}

/// FU-8f phase G6a — the merge gate's retained-load-transpose accounting.
///
/// `rounds` counts every gate-batch load build the retention slot observed
/// (one per `evaluate_gate_batch` call, including the first build of a
/// tournament, whose slot is still empty).  `rebuilds` counts **guard
/// misses** — rounds where a retention guard (mesh fingerprint, contact
/// witness, merged-resolves-specialized poison, bucket cardinality) refused
/// reuse and the round fell back to a counted full rebuild.  A guard miss is
/// never silent: the design record (`fu8f-g6-design-2026-08-07.md`, D-G6-3)
/// requires every miss to land here.  In phase G6a every round executes the
/// full build regardless (rebuild-always), so `rebuilds` is purely the guard
/// verdict; from G6b on it is also the count of rounds that paid the rebuild.
/// `by_cause` carries one label per miss kind so a nonzero total is
/// attributable without a rerun.
#[derive(Clone, Debug, Default)]
pub struct RetainedLoadCensus {
    pub rounds: usize,
    pub rebuilds: usize,
    pub by_cause: std::collections::BTreeMap<&'static str, usize>,
}

/// FU-8c observation O1 — how much merge-proposal work each model actually
/// offers the specialized gate.
///
/// FU-8c replaced the gate's per-proposal re-evaluation of the whole contact
/// set with a per-proposal route-key delta, and the delta path's equivalence to
/// its predecessor is proved on a seven-proposal fixture. Whether any corpus
/// model other than the Weeping Treant *exercises* that path on real geometry
/// was an open question the records could not answer, because nothing counted
/// proposals: a model with zero merge proposals never enters the loop at all.
/// This counts them so the answer is measured rather than assumed.
///
/// Accumulated over **every** `evaluate_gate_batch` call the model makes, not
/// kept per widest batch like [`ScaledCapRow`]: the delta path runs once per
/// proposal per batch, so "does this model exercise it" is a total, not a
/// maximum. `batches` is carried alongside so a zero `proposals` can be told
/// apart from a model that never reached the gate — the latter records no gate
/// batch at all and prints no row.
#[derive(Clone, Copy, Debug, Default)]
pub struct GateProposalCensus {
    pub batches: usize,
    pub proposals: usize,
}

/// One materialization of a batch-product-scaled execution cap.
///
/// The no-silent-scaled-caps rule (owner ruling R1) requires every cap whose
/// bound becomes model-scaled to be observable, not merely documented: this row
/// is how the census reports what bound a model's own batch actually got.
#[derive(Clone, Copy, Debug)]
pub struct ScaledCapRow {
    pub kind: &'static str,
    pub floor: usize,
    pub per_batch_unit: usize,
    pub rows: usize,
    pub columns: usize,
    pub materialized: usize,
}

fn data() -> &'static Mutex<CensusData> {
    static DATA: OnceLock<Mutex<CensusData>> = OnceLock::new();
    DATA.get_or_init(|| Mutex::new(CensusData::default()))
}

fn with<R>(body: impl FnOnce(&mut CensusData) -> R) -> Option<R> {
    if !enabled() {
        return None;
    }
    let mut guard = data().lock().ok()?;
    Some(body(&mut guard))
}

pub fn reset() {
    with(|census| {
        census.rung_outcomes.clear();
        census.rungs.clear();
        census.embeds.clear();
        census.anchors.clear();
        census.segs.clear();
        census.contacts.clear();
        census.attestations.clear();
        census.seat_attestations.clear();
        census.appends.clear();
        census.stage_errors.clear();
        census.scaled_caps.clear();
        census.gate_proposals = GateProposalCensus::default();
        census.retained_load = RetainedLoadCensus::default();
    });
}

/// FU-8f phase G6a — one gate-batch load build observed by the retention
/// slot.  `rebuilt_cause` is `Some(label)` exactly when a retention guard
/// missed and the round fell back to a counted full rebuild.
pub(crate) fn record_retained_load_build(rebuilt_cause: Option<&'static str>) {
    with(|census| {
        census.retained_load.rounds = census.retained_load.rounds.saturating_add(1);
        if let Some(cause) = rebuilt_cause {
            census.retained_load.rebuilds = census.retained_load.rebuilds.saturating_add(1);
            let counted = census.retained_load.by_cause.entry(cause).or_insert(0);
            *counted = counted.saturating_add(1);
        }
    });
}

/// FU-8c observation O1 — one specialized gate batch's merge-proposal count.
/// Recorded at the top of `evaluate_gate_batch`, beside the scaled-cap row and
/// before any charge can refuse, so a model that is refused at this gate still
/// reports what it offered.
pub(crate) fn record_gate_proposals(proposals: usize) {
    with(|census| {
        census.gate_proposals.batches = census.gate_proposals.batches.saturating_add(1);
        census.gate_proposals.proposals = census.gate_proposals.proposals.saturating_add(proposals);
    });
}

/// PR-8 — one attestation verdict per initial contact stub.
#[allow(clippy::fn_params_excessive_bools, reason = "throwaway census row")]
pub(crate) fn record_contact_attestation(
    terminal: u32,
    centreline: bool,
    rim_engaged: usize,
    rim_total: usize,
    attested_radius_mm: f64,
    refused: bool,
    area_deficit: bool,
) {
    with(|census| {
        census.attestations.push(AttestationRow {
            terminal,
            centreline,
            rim_engaged,
            rim_total,
            attested_radius_mm,
            refused,
            area_deficit,
        });
    });
}

/// F3 — one seat-attestation verdict per candidate of a touching family.
#[allow(clippy::fn_params_excessive_bools, reason = "throwaway census row")]
pub(crate) fn record_seat_attestation(
    family: &'static str,
    centreline: bool,
    rim_engaged: usize,
    rim_total: usize,
    attested_radius_mm: f64,
    refused: bool,
    area_deficit: bool,
) {
    with(|census| {
        census.seat_attestations.push(SeatAttestationRow {
            family,
            centreline,
            rim_engaged,
            rim_total,
            attested_radius_mm,
            refused,
            area_deficit,
        });
    });
}

/// Records one materialized batch-product-scaled cap. Only the widest
/// materialization per kind is kept, which is the one that governs whether the
/// model is refused.
///
/// **Ties keep the first, not the widest batch** (documented 2026-08-06,
/// record fix F11). The row this keeps is selected on `materialized` alone,
/// and `materialized` saturates at the cap's floor: while a model's batches
/// are all small enough to sit at the floor — which is every corpus model
/// today, for both kinds — every checkpoint materializes the *same* number,
/// the `>=` arm below wins every time, and the row that survives is the
/// **first** checkpoint recorded rather than the one with the largest
/// `rows`/`columns`. So the printed `rows=`/`columns=` are not the model's
/// widest batch shape whenever the floor governs; only `materialized` is
/// authoritative under a tie.
///
/// That is deliberate and adequate for what the census claims — the governing
/// bound, and whether it is still the floor — but it means the census row must
/// not be read as a batch-shape measurement. `CORPUS_LOAD_BATCHES` in
/// `crates/plinth-support/tests/resource_cap_policy.rs` pins the widest
/// instrumented shapes instead, and differs from the census for exactly this
/// reason on the Cylinder (66 rows pinned, 61 printed) and
/// `make_overhang_table` (12 pinned, 10 printed); see that docstring for the
/// full per-model checkpoint table. Comparing on `rows * columns` instead
/// would break the tie by width, but it would also stop the kept row being the
/// one that states the governing bound; the choice has not been made lightly
/// and should not be changed without re-pinning both.
pub(crate) fn record_scaled_cap(entry: ScaledCapRow) {
    with(|census| {
        match census
            .scaled_caps
            .iter_mut()
            .find(|existing| existing.kind == entry.kind)
        {
            // `>=`, not `>`: on a tie the incumbent stays. See the tie note above.
            Some(existing) if existing.materialized >= entry.materialized => {}
            Some(existing) => *existing = entry,
            None => census.scaled_caps.push(entry),
        }
    });
}

fn report_scaled_caps(model: &str, census: &CensusData) {
    for cap in &census.scaled_caps {
        let mut cells = vec![
            format!("kind={}", cap.kind),
            format!("floor={}", cap.floor),
            format!("per_batch_unit={}", cap.per_batch_unit),
            format!("rows={}", cap.rows),
            format!("columns={}", cap.columns),
            format!("materialized={}", cap.materialized),
            format!("at_floor={}", cap.materialized == cap.floor),
        ];
        // FU-8c observation O1, additive: the gate row alone carries the
        // model's merge-proposal census. It rides here rather than on a row of
        // its own because these are the same batches under the same cap, and
        // because this row is printed on the failure path too -- a model
        // refused at this gate still reports what it offered. Unlike the
        // `rows=`/`columns=` above, which are the retained batch's shape, these
        // two are totals over every gate batch the model ran; see
        // [`GateProposalCensus`].
        if cap.kind == "specialized_gate_operations" {
            cells.push(format!("gate_batches={}", census.gate_proposals.batches));
            cells.push(format!(
                "merge_proposals={}",
                census.gate_proposals.proposals
            ));
            // FU-8f phase G6a, additive like the two cells above: the gate row
            // alone carries the retained-load-transpose accounting, because
            // these are the same batches.  Totals over every gate batch; see
            // [`RetainedLoadCensus`] for what a "rebuild" means per phase.
            cells.push(format!(
                "retained_load_rounds={}",
                census.retained_load.rounds
            ));
            cells.push(format!(
                "retained_load_rebuilds={}",
                census.retained_load.rebuilds
            ));
            for (cause, count) in &census.retained_load.by_cause {
                cells.push(format!("retained_load_rebuild_{cause}={count}"));
            }
        }
        row(model, "T0_scaled_cap", &cells);
    }
}

pub(crate) fn note_stage_error<E: std::fmt::Debug>(stage: &str, error: &E) {
    with(|census| census.stage_errors.push(format!("{stage}: {error:?}")));
}

pub(crate) fn record_rung(
    abs_cos_raw: f64,
    cos_used: f64,
    length_mm: f64,
    radius_mm: f64,
    inset1_mm: f64,
) {
    with(|census| {
        census.rungs.push(RungRow {
            abs_cos_raw,
            cos_used,
            length_mm,
            radius_mm,
            inset1_mm,
        });
    });
}

pub(crate) fn record_embed(
    abs_cos_raw: f64,
    cos_used: f64,
    radius_mm: f64,
    inset_mm: f64,
    segment_len_mm: f64,
    kind: &MemberKind,
    end: ContactEnd,
) {
    with(|census| {
        census.embeds.push(EmbedRow {
            abs_cos_raw,
            cos_used,
            radius_mm,
            inset_mm,
            segment_len_mm,
            kind: format!("{kind:?}"),
            end: match end {
                ContactEnd::Terminalward => "terminalward",
                ContactEnd::Rootward => "rootward",
            },
        });
    });
}

/// Batched: one call per brace candidate-clearance batch.
pub(crate) fn record_rung_outcomes(rows: impl Iterator<Item = RungOutcome>) {
    with(|census| census.rung_outcomes.extend(rows));
}

pub(crate) fn record_append(label: &'static str) {
    with(|census| *census.appends.entry(label).or_default() += 1);
}

pub(crate) fn record_model_anchor(stub_end: [f64; 3], seat: [f64; 3]) {
    with(|census| census.anchors.push(AnchorRow { stub_end, seat }));
}

/// Batched: one call per `segment_clearance` batch, never per row.
pub(crate) fn record_segment_batch(
    site: &'static str,
    p0s: &[[f64; 3]],
    p1s: &[[f64; 3]],
    inset0s: &[f64],
    inset1s: &[f64],
    cos_class: &[u8],
) {
    with(|census| {
        for index in 0..p0s.len() {
            let delta = [
                p1s[index][0] - p0s[index][0],
                p1s[index][1] - p0s[index][1],
                p1s[index][2] - p0s[index][2],
            ];
            let length = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
            // Exactly `seg_clear_one`'s two INFINITY predicates.
            let degenerate = length < 1e-9;
            let usable = (length - inset1s[index]) - inset0s[index];
            census.segs.push(SegRow {
                site,
                segment_len_mm: length,
                inset0_mm: inset0s[index],
                inset1_mm: inset1s[index],
                cos_class: cos_class.get(index).copied().unwrap_or(0),
                has_embed_inset: inset0s[index] != 0.0 || inset1s[index] != 0.0,
                degenerate,
                over_masked: !degenerate && usable <= 1e-9,
            });
        }
    });
}

/// Batched: one call per initial-contact seeding pass.
pub(crate) fn record_contacts(rows: impl Iterator<Item = ([f64; 3], [f64; 3], f64)>) {
    with(|census| {
        for (position, normal, tip_radius_mm) in rows {
            census.contacts.push(ContactRow {
                position,
                normal,
                tip_radius_mm,
            });
        }
    });
}

// ---------------------------------------------------------------------------
// Reporting
// ---------------------------------------------------------------------------

const COS_EDGES: [f64; 4] = [0.3, 0.5, 0.7, 0.9];
const COS_LABELS: [&str; 5] = ["<0.3", "0.3-0.5", "0.5-0.7", "0.7-0.9", ">=0.9"];

fn bucket(abs_cos: f64) -> usize {
    COS_EDGES.iter().filter(|edge| abs_cos >= **edge).count()
}

fn histogram(values: impl Iterator<Item = f64>) -> [usize; 5] {
    let mut counts = [0usize; 5];
    for value in values {
        counts[bucket(value)] += 1;
    }
    counts
}

fn row(model: &str, table: &str, cells: &[String]) {
    println!("CENSUS\t{table}\t{model}\t{}", cells.join("\t"));
}

fn stat(values: &mut [f64]) -> (f64, f64, f64, f64) {
    values.sort_by(f64::total_cmp);
    let count = values.len();
    if count == 0 {
        return (f64::NAN, f64::NAN, f64::NAN, f64::NAN);
    }
    let sum: f64 = values.iter().sum();
    (
        values[0],
        values[count / 2],
        values[count - 1],
        sum / count as f64,
    )
}

/// Drain the census for one model, resolve every deferred normal in single
/// batched native calls, and print the per-model tables.
pub fn finish(model: &str, snapshot: &SupportVerificationSnapshot<'_>) {
    if !enabled() {
        return;
    }
    let Ok(census) = data().lock() else {
        return;
    };
    let surface = snapshot.native_surface();

    // --- Table 1: brace model-rung incidence ------------------------------
    let counts = histogram(census.rungs.iter().map(|entry| entry.abs_cos_raw));
    let grazing = census
        .rungs
        .iter()
        .filter(|entry| entry.abs_cos_raw < 0.5)
        .collect::<Vec<_>>();
    let short_band = grazing
        .iter()
        .filter(|entry| entry.length_mm <= FLOORED_RUNG_INSET_MM)
        .count();
    row(
        model,
        "T1_rung_cos",
        &[
            format!("total={}", census.rungs.len()),
            format!("{}={}", COS_LABELS[0], counts[0]),
            format!("{}={}", COS_LABELS[1], counts[1]),
            format!("{}={}", COS_LABELS[2], counts[2]),
            format!("{}={}", COS_LABELS[3], counts[3]),
            format!("{}={}", COS_LABELS[4], counts[4]),
            format!("grazing_lt_0.5={}", grazing.len()),
            format!("grazing_L_le_2.50mm={short_band}"),
            format!("grazing_L_gt_2.50mm={}", grazing.len() - short_band),
            format!("legal_band_mm={:?}", RUNG_BAND_MM),
        ],
    );

    // --- Table 1a: what happened to those rung candidates -----------------
    {
        let total = census.rung_outcomes.len();
        let grazing = census
            .rung_outcomes
            .iter()
            .filter(|entry| entry.cos_used <= 0.5 + 1e-9)
            .collect::<Vec<_>>();
        let admitted = census
            .rung_outcomes
            .iter()
            .filter(|entry| entry.viable)
            .count();
        let grazing_admitted = grazing.iter().filter(|entry| entry.viable).count();
        let over_masked = census
            .rung_outcomes
            .iter()
            .filter(|entry| entry.over_masked)
            .count();
        let grazing_over_masked = grazing.iter().filter(|entry| entry.over_masked).count();
        let infinite_clearance = census
            .rung_outcomes
            .iter()
            .filter(|entry| entry.clearance_mm.is_infinite())
            .count();
        row(
            model,
            "T1a_rung_outcome",
            &[
                format!("rows={total}"),
                // Renamed for Phase B-2: nothing is floored any more, so this
                // counts rows whose SEATED incidence is still grazing.
                format!("seated_grazing_cos={}", grazing.len()),
                format!("admitted_viable={admitted}"),
                format!("grazing_admitted={grazing_admitted}"),
                format!("over_masked={over_masked}"),
                format!("grazing_over_masked={grazing_over_masked}"),
                format!("clearance_infinite={infinite_clearance}"),
            ],
        );
    }

    // --- Table 1r: every model-rung row, verbatim -------------------------
    for (index, entry) in census.rungs.iter().enumerate() {
        row(
            model,
            "T1r_rung_row",
            &[
                format!("i={index}"),
                format!("abs_cos_raw={:.6}", entry.abs_cos_raw),
                format!("cos_used={:.6}", entry.cos_used),
                // Renamed for Phase B-2. This column used to mean "the cosine
                // floor was active"; the floor is gone, and `cos_used` now
                // differs from `abs_cos_raw` because of SEATING, which RAISES
                // the incidence. Reporting that as "floored" would have made
                // the delta instrument lie about the very change it measures.
                format!("raw_grazing={}", entry.abs_cos_raw < 0.5),
                format!("L_mm={:.6}", entry.length_mm),
                format!("radius_mm={:.6}", entry.radius_mm),
                format!("inset1_mm={:.6}", entry.inset1_mm),
                format!("usable_mm={:.6}", entry.length_mm - entry.inset1_mm),
            ],
        );
    }

    // --- Table 1b: member-level embed incidence (exact clearance) ---------
    let mut by_kind = std::collections::BTreeMap::<(String, &'static str), Vec<f64>>::new();
    for entry in &census.embeds {
        by_kind
            .entry((entry.kind.clone(), entry.end))
            .or_default()
            .push(entry.abs_cos_raw);
    }
    for ((kind, end), values) in &by_kind {
        let counts = histogram(values.iter().copied());
        let grazing = values.iter().filter(|value| **value < 0.5).count();
        row(
            model,
            "T1b_embed_cos",
            &[
                format!("kind={kind}"),
                format!("end={end}"),
                format!("total={}", values.len()),
                format!("{}={}", COS_LABELS[0], counts[0]),
                format!("{}={}", COS_LABELS[1], counts[1]),
                format!("{}={}", COS_LABELS[2], counts[2]),
                format!("{}={}", COS_LABELS[3], counts[3]),
                format!("{}={}", COS_LABELS[4], counts[4]),
                format!("grazing_lt_0.5={grazing}"),
            ],
        );
    }

    // --- Table 2: CCO model-anchor struts ---------------------------------
    // Attempted population: every strut `apply_model_anchor` constructed
    // (candidate evaluation included). One batched projection resolves all
    // seat normals.
    let seats = census
        .anchors
        .iter()
        .map(|entry| entry.seat)
        .collect::<Vec<_>>();
    if !seats.is_empty() {
        let (_, _, _, normals) = surface.project(&seats);
        let mut cosines = Vec::with_capacity(seats.len());
        for (entry, normal) in census.anchors.iter().zip(normals) {
            let axis = [
                entry.seat[0] - entry.stub_end[0],
                entry.seat[1] - entry.stub_end[1],
                entry.seat[2] - entry.stub_end[2],
            ];
            let length = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
            if length <= 0.0 {
                continue;
            }
            let dot = (axis[0] * normal[0] + axis[1] * normal[1] + axis[2] * normal[2]) / length;
            cosines.push(dot.abs());
        }
        let counts = histogram(cosines.iter().copied());
        let grazing = cosines.iter().filter(|value| **value < 0.5).count();
        row(
            model,
            "T2_cco_anchor_attempted",
            &[
                format!("total={}", cosines.len()),
                format!("{}={}", COS_LABELS[0], counts[0]),
                format!("{}={}", COS_LABELS[1], counts[1]),
                format!("{}={}", COS_LABELS[2], counts[2]),
                format!("{}={}", COS_LABELS[3], counts[3]),
                format!("{}={}", COS_LABELS[4], counts[4]),
                format!("grazing_lt_0.5={grazing}"),
            ],
        );
    } else {
        row(model, "T2_cco_anchor_attempted", &["total=0".to_owned()]);
    }

    // Surviving population: struts seated on the model in the sealed snapshot.
    let surviving = snapshot
        .final_live_members()
        .filter(|member| {
            *member.kind() == MemberKind::Strut
                && member.model_contact_end() == Some(ContactEnd::Rootward)
        })
        .map(|member| (member.p0_mm(), member.p1_mm()))
        .collect::<Vec<_>>();
    if surviving.is_empty() {
        row(model, "T2_cco_anchor_surviving", &["total=0".to_owned()]);
    } else {
        let seats = surviving.iter().map(|(_, p1)| *p1).collect::<Vec<_>>();
        let (_, _, _, normals) = surface.project(&seats);
        let mut cosines = Vec::with_capacity(seats.len());
        for ((p0, p1), normal) in surviving.iter().zip(normals) {
            let axis = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
            let length = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
            if length <= 0.0 {
                continue;
            }
            let dot = (axis[0] * normal[0] + axis[1] * normal[1] + axis[2] * normal[2]) / length;
            cosines.push(dot.abs());
        }
        let counts = histogram(cosines.iter().copied());
        let grazing = cosines.iter().filter(|value| **value < 0.5).count();
        row(
            model,
            "T2_cco_anchor_surviving",
            &[
                format!("total={}", cosines.len()),
                format!("{}={}", COS_LABELS[0], counts[0]),
                format!("{}={}", COS_LABELS[1], counts[1]),
                format!("{}={}", COS_LABELS[2], counts[2]),
                format!("{}={}", COS_LABELS[3], counts[3]),
                format!("{}={}", COS_LABELS[4], counts[4]),
                format!("grazing_lt_0.5={grazing}"),
            ],
        );
    }

    // --- Table 3: over-mask admissions ------------------------------------
    let mut sites = std::collections::BTreeMap::<&'static str, [usize; 9]>::new();
    for entry in &census.segs {
        // [rows, embed_rows, cos_rows, floored_rows, over_masked,
        //  over_masked_floored, over_masked_cos_ge, over_masked_no_cos,
        //  degenerate]
        let slot = sites.entry(entry.site).or_default();
        slot[0] += 1;
        if entry.has_embed_inset {
            slot[1] += 1;
        }
        if entry.cos_class != 0 {
            slot[2] += 1;
        }
        if entry.cos_class == 1 {
            slot[3] += 1;
        }
        if entry.over_masked {
            slot[4] += 1;
            match entry.cos_class {
                1 => slot[5] += 1,
                2 => slot[6] += 1,
                _ => slot[7] += 1,
            }
        }
        if entry.degenerate {
            slot[8] += 1;
        }
    }
    for (site, slot) in &sites {
        row(
            model,
            "T3_over_mask",
            &[
                format!("site={site}"),
                format!("rows={}", slot[0]),
                format!("embed_inset_rows={}", slot[1]),
                format!("cos_derived_rows={}", slot[2]),
                format!("grazing_cos_rows={}", slot[3]),
                format!("over_masked={}", slot[4]),
                format!("over_masked_floored_cos={}", slot[5]),
                format!("over_masked_cos_ge_0.5={}", slot[6]),
                format!("over_masked_no_cos_inset={}", slot[7]),
                format!("degenerate_len={}", slot[8]),
            ],
        );
    }

    // --- Table 3m: how close the embed-inset rows come to over-masking ----
    let mut margins = std::collections::BTreeMap::<&'static str, Vec<(f64, f64)>>::new();
    for entry in &census.segs {
        if !entry.has_embed_inset || entry.degenerate {
            continue;
        }
        let usable = (entry.segment_len_mm - entry.inset1_mm) - entry.inset0_mm;
        margins
            .entry(entry.site)
            .or_default()
            .push((usable, usable / entry.segment_len_mm));
    }
    for (site, values) in &margins {
        let mut absolute = values.iter().map(|(value, _)| *value).collect::<Vec<_>>();
        let mut ratio = values.iter().map(|(_, value)| *value).collect::<Vec<_>>();
        let (min_abs, median_abs, max_abs, _) = stat(&mut absolute);
        let (min_ratio, median_ratio, _, _) = stat(&mut ratio);
        row(
            model,
            "T3m_embed_margin",
            &[
                format!("site={site}"),
                format!("embed_inset_rows={}", values.len()),
                format!("min_usable_mm={min_abs:.6}"),
                format!("median_usable_mm={median_abs:.6}"),
                format!("max_usable_mm={max_abs:.6}"),
                format!("min_usable_fraction={min_ratio:.6}"),
                format!("median_usable_fraction={median_ratio:.6}"),
            ],
        );
    }

    // --- Table 4: embedded-stub tip radii + local curvature ---------------
    let mut radii = census
        .contacts
        .iter()
        .map(|entry| entry.tip_radius_mm)
        .collect::<Vec<_>>();
    let mut distinct = std::collections::BTreeMap::<u64, usize>::new();
    for value in &radii {
        *distinct.entry(value.to_bits()).or_default() += 1;
    }
    let (min, median, max, mean) = stat(&mut radii);
    let mut cells = vec![
        format!("total={}", census.contacts.len()),
        format!("min={min:.6}"),
        format!("median={median:.6}"),
        format!("max={max:.6}"),
        format!("mean={mean:.6}"),
    ];
    for (bits, count) in &distinct {
        cells.push(format!("r={:.6}:{count}", f64::from_bits(*bits)));
    }
    row(model, "T4_tip_radii", &cells);

    // Curvature probe: four tangent samples per contact at the tip radius,
    // resolved in ONE batched projection for the whole model.
    if !census.contacts.is_empty() {
        let mut probes = Vec::with_capacity(census.contacts.len() * 4);
        for entry in &census.contacts {
            let normal = entry.normal;
            let helper = if normal[2].abs() < 0.9 {
                [0.0, 0.0, 1.0]
            } else {
                [1.0, 0.0, 0.0]
            };
            let mut u = [
                normal[1] * helper[2] - normal[2] * helper[1],
                normal[2] * helper[0] - normal[0] * helper[2],
                normal[0] * helper[1] - normal[1] * helper[0],
            ];
            let un = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt();
            for axis in &mut u {
                *axis /= un;
            }
            let v = [
                normal[1] * u[2] - normal[2] * u[1],
                normal[2] * u[0] - normal[0] * u[2],
                normal[0] * u[1] - normal[1] * u[0],
            ];
            let step = entry.tip_radius_mm.max(0.05);
            for (sign_u, sign_v) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                probes.push(std::array::from_fn(|axis| {
                    entry.position[axis] + step * (sign_u * u[axis] + sign_v * v[axis])
                }));
            }
        }
        let (_, projected, _, _) = surface.project(&probes);
        let mut flat = 0usize;
        let mut convex = 0usize;
        let mut concave = 0usize;
        let mut saddle = 0usize;
        for (index, entry) in census.contacts.iter().enumerate() {
            let tolerance = 0.02 * entry.tip_radius_mm.max(0.05);
            let mut positive = 0usize;
            let mut negative = 0usize;
            for probe in 0..4 {
                let point = projected[index * 4 + probe];
                let height = (0..3)
                    .map(|axis| (point[axis] - entry.position[axis]) * entry.normal[axis])
                    .sum::<f64>();
                if height > tolerance {
                    positive += 1;
                } else if height < -tolerance {
                    negative += 1;
                }
            }
            match (positive, negative) {
                (0, 0) => flat += 1,
                (0, _) => convex += 1,
                (_, 0) => concave += 1,
                _ => saddle += 1,
            }
        }
        row(
            model,
            "T4_curvature",
            &[
                format!("total={}", census.contacts.len()),
                format!("flat={flat}"),
                format!("convex={convex}"),
                format!("concave={concave}"),
                format!("saddle={saddle}"),
            ],
        );
    }

    // --- Table 5: final live member composition ---------------------------
    let mut composition = std::collections::BTreeMap::<(String, &'static str), usize>::new();
    for member in snapshot.final_live_members() {
        let end = match member.model_contact_end() {
            None => "none",
            Some(ContactEnd::Terminalward) => "terminalward",
            Some(ContactEnd::Rootward) => "rootward",
        };
        *composition
            .entry((format!("{:?}", member.kind()), end))
            .or_default() += 1;
    }
    let cells = composition
        .iter()
        .map(|((kind, end), count)| format!("{kind}/{end}={count}"))
        .collect::<Vec<_>>();
    row(model, "T5_final_members", &cells);

    let cells = census
        .appends
        .iter()
        .map(|(label, count)| format!("{label}={count}"))
        .collect::<Vec<_>>();
    row(model, "T6_commits", &cells);

    // --- Table 7: PR-8 contact attestation --------------------------------
    if !census.attestations.is_empty() {
        let total = census.attestations.len();
        let refused = census.attestations.iter().filter(|a| a.refused).count();
        let deficits = census
            .attestations
            .iter()
            .filter(|a| a.area_deficit)
            .count();
        let full = census
            .attestations
            .iter()
            .filter(|a| a.centreline && a.rim_engaged == a.rim_total)
            .count();
        let no_centre = census.attestations.iter().filter(|a| !a.centreline).count();
        let min_engaged = census
            .attestations
            .iter()
            .map(|a| a.rim_engaged)
            .min()
            .unwrap_or(0);
        row(
            model,
            "T7_attestation",
            &[
                format!("total={total}"),
                format!("fully_engaged={full}"),
                format!("centreline_outside={no_centre}"),
                format!("min_rim_engaged={min_engaged}"),
                format!("refused={refused}"),
                format!("area_deficit={deficits}"),
            ],
        );
        for attestation in census.attestations.iter().filter(|a| {
            a.refused || a.area_deficit || a.rim_engaged != a.rim_total || !a.centreline
        }) {
            row(
                model,
                "T7r_attestation_row",
                &[
                    format!("terminal={}", attestation.terminal),
                    format!("centreline={}", attestation.centreline),
                    format!("rim={}/{}", attestation.rim_engaged, attestation.rim_total),
                    format!("r_eff_mm={:.6}", attestation.attested_radius_mm),
                    format!("refused={}", attestation.refused),
                    format!("area_deficit={}", attestation.area_deficit),
                ],
            );
        }
    }

    // --- Table 8: F3 seat attestation for the TOUCHING families -----------
    //
    // One row per family so the two are never conflated: a brace model rung is
    // seated by PR-1 and a CCO model anchor is not (PR-7 is deliberately not
    // landed), so they are expected to measure differently and the instrument
    // must be able to say so.
    for family in ["brace_model_rung", "cco_model_anchor"] {
        let rows = census
            .seat_attestations
            .iter()
            .filter(|seat| seat.family == family)
            .collect::<Vec<_>>();
        if rows.is_empty() {
            continue;
        }
        let refused = rows.iter().filter(|seat| seat.refused).count();
        let deficits = rows.iter().filter(|seat| seat.area_deficit).count();
        let full = rows
            .iter()
            .filter(|seat| seat.centreline && seat.rim_engaged == seat.rim_total)
            .count();
        let no_centre = rows.iter().filter(|seat| !seat.centreline).count();
        let min_engaged = rows.iter().map(|seat| seat.rim_engaged).min().unwrap_or(0);
        row(
            model,
            "T8_seat_attestation",
            &[
                format!("family={family}"),
                format!("total={}", rows.len()),
                format!("fully_engaged={full}"),
                format!("centreline_outside={no_centre}"),
                format!("min_rim_engaged={min_engaged}"),
                format!("refused={refused}"),
                format!("area_deficit={deficits}"),
            ],
        );
        for (index, seat) in rows.iter().enumerate() {
            row(
                model,
                "T8r_seat_attestation_row",
                &[
                    format!("family={family}"),
                    format!("i={index}"),
                    format!("centreline={}", seat.centreline),
                    format!("rim={}/{}", seat.rim_engaged, seat.rim_total),
                    format!("r_eff_mm={:.6}", seat.attested_radius_mm),
                    format!("refused={}", seat.refused),
                    format!("area_deficit={}", seat.area_deficit),
                ],
            );
        }
    }

    report_scaled_caps(model, &census);
    for error in &census.stage_errors {
        row(model, "T0_stage_error", std::slice::from_ref(error));
    }
}

/// Print any stage errors recorded for a model that never reached `finish`.
pub fn report_failure(model: &str) {
    if !enabled() {
        return;
    }
    let Ok(census) = data().lock() else {
        return;
    };
    report_scaled_caps(model, &census);
    for error in &census.stage_errors {
        row(model, "T0_stage_error", std::slice::from_ref(error));
    }
}
