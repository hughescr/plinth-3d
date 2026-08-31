//! I4 load ownership -- `docs/specs/audit-invariants.md` §3.4.
//!
//! Every unit of carried model area has exactly one owner: none
//! double-counted, none unassigned. The invariant is graded on **one item per
//! model** -- the model's ownership partition -- through five clauses, all
//! gating:
//!
//! | clause | statement | source |
//! |---|---|---|
//! | (a) | `A_unowned <= pixel_area` | §3.4 (a) |
//! | (b) | the partition identity `|accounted - D_total| <= tol` | §3.4 (b) |
//! | (c) | every contact routed to exactly one surface component, owning samples from no other | §3.4 (c) |
//! | (d) | the engine's books bracket the reconstructed demand | §3.4 (d) |
//! | (e) | `D_total` reconstructed a second way from the raster and required to agree | §2.1 |
//!
//! ## Three places this file departs from a literal reading, each on purpose
//!
//! 1. **Clause (b) is graded on `accounted = sum A_owned + A_unowned`, not on
//!    `sum A_owned` alone.** Read literally, §3.4 (b) compares `sum A_owned`
//!    against `D_total`, and by §2.2 the difference between those two is
//!    *exactly* `A_unowned` -- unowned samples arise only on components that
//!    carry no contact at all, so within a component that has a contact every
//!    sample is owned. The literal clause is therefore not an identity check
//!    at all: it is clause (a) again at a 10^5-times tighter tolerance, and a
//!    file with genuinely unassigned area would be reported as
//!    `partition_identity_violated` -- an *instrument* fault -- when the
//!    instrument worked perfectly and the plan is the thing at fault. Its own
//!    prose says what it is for ("a Voronoi partition cannot double-count, so
//!    a violation means the sampler or the component routing is wrong"), and
//!    `Ownership::accounted_mm2` is documented for exactly this comparison.
//!    **No strictness is lost.** Any `A_unowned > 0` comes by construction
//!    from at least one component with demand and no contact, which raises
//!    `LOAD_COMPONENT_UNSUPPORTED` at *zero* censor and fails the invariant.
//!    The literal residual is computed anyway and published as
//!    `measured.b_literal_residual_mm2` with `b_literal_state`, so a reader
//!    can grade the literal clause from the output without rerunning anything.
//! 2. **Clause (e) raises `FILE_MESH_NOT_CLOSED` from inside a check.** §2.1
//!    makes the demand double-reconstruction a file-level failure, but the
//!    driver's file-level abort path (`run::audit_file`) does not compute it
//!    and this agent owns exactly one file. Emitting it here keeps the code
//!    and the reason token the specification names; the consequence is that it
//!    lands in `i4_load_ownership.failures` rather than in
//!    `verdict.file_level_failures`.
//! 3. **Two `reason=` tokens beyond the three §3.4 enumerates.**
//!    `tip_radius_not_established` (the three-way tip-radius comparison of
//!    §3.1 disagrees, so clause (d)'s inversion has no established input) and
//!    `demand_raster_unavailable` (the model raster errored, so clause (e) has
//!    no second reconstruction). Both are indeterminate, both FAIL. The
//!    alternative to naming them is passing something the audit did not
//!    establish, which rule 3 of §0 forbids.
//!
//! ## Counting
//!
//! `checked = 1`: one ownership partition per model, which is what the §5
//! sample document and the §5 summary line both show for this invariant.
//! `failed` and `indeterminate` are therefore 0 or 1, and indeterminate
//! dominates. Every individual failure line -- one per unsupported component,
//! one per failing clause -- is still present in `failures`, and every number
//! any of them names is in `measured` for every item checked, not only for
//! failures.

use crate::check::{Check, CheckOutcome, Failure};
use crate::checks::interval::{self, Verdict};
use crate::consts as c;
use crate::context::ModelContext;
use crate::json::Json;
use crate::ownership::implied_area;
use crate::raster::{Layer, Run, RunRaster};
use crate::reader::RecordKind;
use crate::vec3::V3;
use std::collections::BTreeMap;

/// How many `LOAD_COMPONENT_UNSUPPORTED` lines are written before the rest are
/// summarised by count. The components are sorted by demand descending first,
/// so the cap never hides the worst one. `measured.components_without_contact`
/// always carries the full count.
const MAX_COMPONENT_LINES: usize = 64;
/// How many rows the `measured.components` array carries, worst-demand first.
const MAX_COMPONENT_ROWS: usize = 128;

// ---------------------------------------------------------------------------
// The predicates, pure and separately testable
// ---------------------------------------------------------------------------

/// What one clause established. There is no fourth value, and
/// [`ClauseState::Indeterminate`] is a FAILURE with a reason, never a silence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClauseState {
    Pass,
    Fail,
    Indeterminate,
}

impl ClauseState {
    /// Lift an interval comparison's verdict into this check's own
    /// three-state vocabulary. The two enums are the same three outcomes
    /// under different names; this is the only place they meet, so a
    /// `Verdict::Indeterminate` cannot be flattened into a pass on the way
    /// across.
    #[must_use]
    pub const fn from_verdict(v: Verdict) -> Self {
        match v {
            Verdict::Pass => Self::Pass,
            Verdict::Fail => Self::Fail,
            Verdict::Indeterminate => Self::Indeterminate,
        }
    }
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Indeterminate => "indeterminate",
        }
    }
    #[must_use]
    pub const fn is_pass(self) -> bool {
        matches!(self, Self::Pass)
    }
}

/// Clause (a): `A_unowned <= pixel_area`.
///
/// Censored at one printer pixel because the printer cannot render less
/// (§1.5). A non-finite input is **unmeasurable**, and an unmeasurable input
/// is indeterminate, never a pass.
///
/// **Conservative MAXIMUM-type form** (`audit-interval-policy.md` §3.3
/// row 11): unowned area must not EXCEED the censor, so the conservative
/// bound of `A_unowned` is its UPPER bound and the pass condition is
/// `A_unowned + tol <= censor`. This is the OPPOSITE bound from the
/// minimum-type rows in this same sweep (I1's depth, I2's patch area, I3's
/// section) — applying one rule to both directions is exactly the inversion
/// the policy warns about.
///
/// `tol` is `tol_sum(A_unowned)`: `A_unowned` is a summed quantity (Source C,
/// §2.3), while the censor is a fixed printer constant with no uncertainty of
/// its own. §2.4 predicts this changes nothing measurable — `tol_sum`'s
/// absolute leg is `1e-9 mm²` against a `2.66e-4 mm²` censor, five decades
/// apart — and it is fixed for correctness regardless.
#[must_use]
pub fn clause_a_unowned(a_unowned_mm2: f64, censor_mm2: f64) -> ClauseState {
    if !a_unowned_mm2.is_finite() || !censor_mm2.is_finite() || censor_mm2 < 0.0 {
        return ClauseState::Indeterminate;
    }
    ClauseState::from_verdict(interval::max_type(
        a_unowned_mm2,
        censor_mm2,
        c::tol_sum(a_unowned_mm2),
    ))
}

/// Clause (b): the partition identity, `|accounted - D_total| <= tol`, with
/// `tol = max(4096*eps64*D_total, 1e-9)`.
///
/// This is the audit auditing itself: a complete partition can neither lose
/// nor duplicate area, so a violation is a defect in the sampler or the
/// routing and must fail rather than pass.
#[must_use]
pub fn clause_b_identity(accounted_mm2: f64, d_total_mm2: f64) -> ClauseState {
    if !accounted_mm2.is_finite() || !d_total_mm2.is_finite() {
        return ClauseState::Indeterminate;
    }
    // EXEMPT, EQUALITY-BAND (`audit-interval-policy.md` §1.4 row 12, §3.4):
    // two independent reconstructions of ONE physical quantity, not a
    // delivered value against a design requirement. There is no safety
    // margin under test, so there is no conservative side to pick and no
    // third outcome to add -- "within combined tolerance" already MEANS "the
    // evidence cannot distinguish these", which is the expected outcome for
    // two correct reconstructions. Converting this to the minimum/maximum
    // interval form would be a category error.
    if (accounted_mm2 - d_total_mm2).abs() <= c::tol_sum(d_total_mm2) {
        ClauseState::Pass
    } else {
        ClauseState::Fail
    }
}

/// Clause (c), as a number rather than as a construction argument.
///
/// The shared partition restricts each sample's candidate set to its own
/// surface component, so "owns samples from no other component" is true there
/// *by construction* -- and a check that grades a construction grades nothing.
/// This grades it against an independently computed quantity instead:
/// `DemandSet::per_component_mm2`, summed straight off the triangles with no
/// sampler involved. For every component carrying at least one contact, the
/// contacts routed to it must own that component's whole demand and no more.
/// A cross-component leak moves area between two components and breaks this
/// identity on both.
///
/// Returns [`ClauseState::Indeterminate`] on any non-finite input, and the
/// index of the worst offender with its residual.
#[must_use]
pub fn clause_c_components(
    owned_by_component: &[f64],
    demand_by_component: &[f64],
    has_contact: &[bool],
) -> (ClauseState, Option<(usize, f64, f64)>) {
    if owned_by_component.len() != demand_by_component.len()
        || has_contact.len() != demand_by_component.len()
    {
        return (ClauseState::Indeterminate, None);
    }
    let mut worst: Option<(usize, f64, f64)> = None;
    let mut state = ClauseState::Pass;
    for k in 0..demand_by_component.len() {
        let (owned, want) = (owned_by_component[k], demand_by_component[k]);
        if !owned.is_finite() || !want.is_finite() {
            return (ClauseState::Indeterminate, Some((k, owned, want)));
        }
        // A component with no contact owns nothing; its demand is unowned and
        // clause (a) and LOAD_COMPONENT_UNSUPPORTED speak to it.
        let expected = if has_contact[k] { want } else { 0.0 };
        // EXEMPT (`audit-interval-policy.md` §1.4 row 13, §3.4): an
        // equality-band comparison of two independent reconstructions of one
        // component's demand, AND it already routes disagreement to
        // `Indeterminate` rather than to `Fail` or `Pass` -- ambiguity never
        // resolves to a pass here. Nothing to correct.
        let resid = (owned - expected).abs();
        let tol = c::tol_sum(expected.max(want));
        if resid > tol {
            state = ClauseState::Indeterminate;
            if worst.is_none_or(|(_, w, _)| resid > w) {
                worst = Some((k, resid, tol));
            }
        }
    }
    (state, worst)
}

/// Clause (d): the engine's books, recovered by inverting
/// `A = x_allowed * pi * r_tip^2`, must bracket the reconstructed demand.
///
/// `lo` sums the **unclamped** tips only; every clamp-limited tip enters
/// through `hi` as an interval of width `x_allowed * pi * r_max^2`, never as a
/// point and never as an exemption (§2.3). A malformed bracket (`hi < lo`) is
/// indeterminate: the instrument, not the plan, is then the thing at fault.
#[must_use]
pub fn clause_d_books(d_total_mm2: f64, lo_mm2: f64, hi_mm2: f64) -> ClauseState {
    if !d_total_mm2.is_finite() || !lo_mm2.is_finite() || !hi_mm2.is_finite() {
        return ClauseState::Indeterminate;
    }
    if hi_mm2 < lo_mm2 {
        return ClauseState::Indeterminate;
    }
    // **Two-sided bracket, needing BOTH rules at once**
    // (`audit-interval-policy.md` §3.3 row 14 -- the sharpest instance in the
    // corpus, because it compounds the bug in two directions on one
    // predicate). `D_total` must clear `lo` from above (a MINIMUM-type
    // relation to `lo`) and clear `hi` from below (a MAXIMUM-type relation to
    // `hi`):
    //
    //   PASS           D - tol >= lo  AND  D + tol <= hi
    //   FAIL           D + tol <  lo  OR   D - tol >  hi
    //   INDETERMINATE  otherwise -- the interval overlaps an end without
    //                  clearing it
    //
    // **Replaces `D >= lo - tol && D <= hi + tol`**, which widened the
    // passing region on both ends simultaneously: it graded `D`'s UPPER
    // bound against `lo` (should be the lower) and `D`'s LOWER bound against
    // `hi` (should be the upper). Both halves were backwards, in opposite
    // senses -- which is why one rule applied twice with the same sign would
    // not have fixed it.
    let tol = c::tol_sum(d_total_mm2);
    ClauseState::from_verdict(interval::bracket(d_total_mm2, lo_mm2, hi_mm2, tol))
}

/// Clause (e), §2.1: `D_total` reconstructed a second way, off the raster, and
/// required to agree within `0.5 * perimeter_total * d2 + tol`.
///
/// The first term is the quantisation bound -- a rasterised boundary can
/// differ from the true one by at most half a pixel diagonal along its length.
#[must_use]
pub fn clause_e_demand_agrees(
    d_total_mm2: f64,
    d_raster_mm2: f64,
    perimeter_total_mm: f64,
) -> ClauseState {
    if !d_total_mm2.is_finite() || !d_raster_mm2.is_finite() || !perimeter_total_mm.is_finite() {
        return ClauseState::Indeterminate;
    }
    if perimeter_total_mm < 0.0 {
        return ClauseState::Indeterminate;
    }
    // EXEMPT, EQUALITY-BAND (`audit-interval-policy.md` §1.4 row 15, §3.4):
    // `D_total` and `D_raster` are the same demand reconstructed two ways.
    // Same reasoning as clause (b) -- symmetric, already conservative, no
    // direction to correct.
    let bound = 0.5 * perimeter_total_mm * c::D2_MM + c::tol_sum(d_total_mm2);
    if (d_total_mm2 - d_raster_mm2).abs() <= bound {
        ClauseState::Pass
    } else {
        ClauseState::Fail
    }
}

// ---------------------------------------------------------------------------
// The second reconstruction of D, off the raster (§2.1)
// ---------------------------------------------------------------------------

/// `D_total` recovered from the printer raster, with the boundary length the
/// quantisation bound is built from.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RasterDemand {
    /// Summed over layers, the area of pixels cured at layer `k` whose own
    /// column is not cured at layer `k-1`. mm^2.
    pub d_raster_mm2: f64,
    /// Summed length of the down-face boundary across layers, mm.
    pub perimeter_total_mm: f64,
    /// Layers that carried any down-face pixel at all.
    pub layers_with_down_face: usize,
    /// Down-face pixels counted, so the resolution behind the number is
    /// visible.
    pub down_face_pixels: u64,
}

/// Subtract one sorted disjoint run list from another. Reimplemented here
/// rather than reached for in [`crate::raster`], whose run algebra is private.
fn subtract_runs(a: &[Run], b: &[Run]) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::with_capacity(a.len() + b.len());
    for r in a {
        let mut c0 = r.c0;
        for cut in b {
            if cut.c1 <= c0 {
                continue;
            }
            if cut.c0 >= r.c1 {
                break;
            }
            if cut.c0 > c0 {
                out.push(Run { c0, c1: cut.c0 });
            }
            c0 = c0.max(cut.c1);
            if c0 >= r.c1 {
                break;
            }
        }
        if c0 < r.c1 {
            out.push(Run { c0, c1: r.c1 });
        }
    }
    out.retain(|r| !r.is_empty());
    out
}

fn run_pixels(runs: &[Run]) -> i64 {
    runs.iter().map(|r| r.len().max(0)).sum()
}

/// Batch-first: one pass over the layers, run-interval algebra per row. A
/// per-pixel loop over a 16K grid is a defect, not an implementation choice
/// (§1.6).
#[must_use]
pub fn raster_demand(r: &RunRaster) -> RasterDemand {
    let (px, py) = (r.grid.pitch_x_mm, r.grid.pitch_y_mm);
    let pixel_area = px * py;
    let mut out = RasterDemand::default();

    for (i, layer) in r.layers.iter().enumerate() {
        // Empty layers are omitted from the vector, so "layer k-1" exists only
        // when the previous entry is literally the previous index.
        let prev: Option<&Layer> = if i > 0 && r.layers[i - 1].k + 1 == layer.k {
            Some(&r.layers[i - 1])
        } else {
            None
        };
        let mut down: BTreeMap<i64, Vec<Run>> = BTreeMap::new();
        for (&row, runs) in &layer.rows {
            let below = prev.map_or(&[][..], |p| p.runs_in_row(row));
            let diff = subtract_runs(runs, below);
            if !diff.is_empty() {
                down.insert(row, diff);
            }
        }
        if down.is_empty() {
            continue;
        }
        out.layers_with_down_face += 1;
        for runs in down.values() {
            let n = run_pixels(runs);
            out.down_face_pixels += n.max(0) as u64;
            out.d_raster_mm2 += n as f64 * pixel_area;
        }
        // Boundary of the down-face region on this layer. Runs within a row
        // are disjoint and never adjacent after the rasteriser normalises, so
        // both ends of every run are boundary; the row-to-row edges are the
        // pixels a neighbouring row does not cover.
        for (&row, runs) in &down {
            out.perimeter_total_mm += 2.0 * py * runs.len() as f64;
            let up = down.get(&(row + 1)).map_or(&[][..], Vec::as_slice);
            let dn = down.get(&(row - 1)).map_or(&[][..], Vec::as_slice);
            let exposed = run_pixels(&subtract_runs(runs, up)) + run_pixels(&subtract_runs(runs, dn));
            out.perimeter_total_mm += px * exposed as f64;
        }
    }

    out
}

// ---------------------------------------------------------------------------
// The check
// ---------------------------------------------------------------------------

pub struct LoadOwnership;

impl Check for LoadOwnership {
    fn id(&self) -> &'static str {
        "i4_load_ownership"
    }

    fn title(&self) -> &'static str {
        "I4 load ownership"
    }

    fn scope(&self, _ctx: &ModelContext) -> usize {
        1
    }

    #[allow(
        clippy::too_many_lines,
        reason = "five gating clauses, their failure lines and the measured \
                  block that must carry every quantity those lines name; \
                  splitting them apart would hide the order they gate in"
    )]
    fn run(&self, ctx: &ModelContext) -> CheckOutcome {
        let file = ctx.file();
        let mi = ctx.model_index;
        let inputs = ctx.inputs();
        let x_allowed = inputs.x_allowed;
        let r_max = inputs.tip_radius_max_mm;

        let mesh = &ctx.model_mesh;
        let demand = ctx.demand();
        let own = ctx.ownership();
        let sites = ctx.contact_sites();

        let mut lines: Vec<Failure> = Vec::new();
        let mut indeterminate = false;
        let mut failed = false;
        let mut measured = Json::obj();

        let d_total = demand.total_mm2;
        let censor = c::PIXEL_AREA_MM2;
        let a_owned_total: f64 = own.a_owned_mm2.iter().sum();

        // -- independent per-component aggregates -------------------------
        //
        // per_component_mm2 comes straight off the triangles with no sampler
        // involved, so comparing the partition against it is a real
        // cross-check and not a restatement of how the partition was built.
        let ncomp = mesh.component_count;
        let mut true_area_by_component = vec![0.0f64; ncomp];
        let mut centroid_acc = vec![([0.0f64; 3], 0.0f64); ncomp];
        for ti in 0..mesh.tri_count() {
            let d = demand.per_triangle_mm2[ti];
            if d <= 0.0 {
                continue;
            }
            let k = mesh.component_of_tri[ti] as usize;
            if k >= ncomp {
                continue;
            }
            true_area_by_component[k] += mesh.areas[ti];
            let ctr = mesh.tri_centroid(ti);
            for ax in 0..3 {
                centroid_acc[k].0[ax] += ctr[ax] * d;
            }
            centroid_acc[k].1 += d;
        }
        let centroid_of = |k: usize| -> V3 {
            let (acc, w) = centroid_acc[k];
            if w > 0.0 {
                [acc[0] / w, acc[1] / w, acc[2] / w]
            } else {
                [0.0; 3]
            }
        };

        let mut owned_by_component = vec![0.0f64; ncomp];
        let mut contacts_by_component = vec![0usize; ncomp];
        let mut unrouted: Vec<usize> = Vec::new();
        for (i, s) in sites.iter().enumerate() {
            match s.component {
                Some(k) if k < ncomp => {
                    owned_by_component[k] += own.a_owned_mm2.get(i).copied().unwrap_or(f64::NAN);
                    contacts_by_component[k] += 1;
                }
                _ => unrouted.push(s.contact_id),
            }
        }
        let has_contact: Vec<bool> = contacts_by_component.iter().map(|&n| n > 0).collect();

        // -- clause (b) and the sample budget ------------------------------
        let budget_hit = own.budget_exceeded;
        let state_b = if budget_hit.is_some() {
            ClauseState::Indeterminate
        } else {
            clause_b_identity(own.accounted_mm2, d_total)
        };
        let b_literal_residual = (a_owned_total - d_total).abs();
        let b_literal_state = clause_b_identity(a_owned_total, d_total);

        if let Some(cap) = budget_hit {
            indeterminate = true;
            lines.push(indet_line(
                &file,
                mi,
                "sample_budget_exceeded",
                &[
                    ("samples", own.samples.to_string()),
                    ("max_samples", cap.to_string()),
                ],
            ));
        } else if state_b != ClauseState::Pass {
            indeterminate = true;
            lines.push(indet_line(
                &file,
                mi,
                "partition_identity_violated",
                &[
                    ("accounted_mm2", num(own.accounted_mm2)),
                    ("d_total_mm2", num(d_total)),
                    ("residual_mm2", num((own.accounted_mm2 - d_total).abs())),
                    ("tolerance_mm2", num(c::tol_sum(d_total))),
                ],
            ));
        }

        // -- clause (c): routing, and no cross-component leak --------------
        let seat_count = ctx.seats.len();
        let tip_records = ctx
            .model
            .records
            .iter()
            .filter(|r| r.kind == RecordKind::ContactSphere)
            .count();
        let mut state_c = if budget_hit.is_some() {
            ClauseState::Indeterminate
        } else {
            ClauseState::Pass
        };
        if seat_count != tip_records {
            state_c = ClauseState::Indeterminate;
            indeterminate = true;
            lines.push(indet_line(
                &file,
                mi,
                "component_routing_failed",
                &[
                    ("seats", seat_count.to_string()),
                    ("tip_records", tip_records.to_string()),
                    ("detail", "seat_count_disagrees_with_tip_record_count".into()),
                ],
            ));
        }
        if !unrouted.is_empty() {
            state_c = ClauseState::Indeterminate;
            indeterminate = true;
            lines.push(indet_line(
                &file,
                mi,
                "component_routing_failed",
                &[
                    ("unrouted_contacts", unrouted.len().to_string()),
                    ("first_contact", unrouted[0].to_string()),
                    ("components", ncomp.to_string()),
                ],
            ));
        }
        if budget_hit.is_none() {
            let (leak_state, worst) =
                clause_c_components(&owned_by_component, &demand.per_component_mm2, &has_contact);
            if leak_state != ClauseState::Pass {
                state_c = ClauseState::Indeterminate;
                indeterminate = true;
                let (k, resid, tol) = worst.unwrap_or((usize::MAX, f64::NAN, f64::NAN));
                lines.push(indet_line(
                    &file,
                    mi,
                    "component_routing_failed",
                    &[
                        ("component", k.to_string()),
                        ("residual_mm2", num(resid)),
                        ("tolerance_mm2", num(tol)),
                        ("detail", "component_owned_total_disagrees_with_its_demand".into()),
                    ],
                ));
            }
            // The shared partition's own list of starved components must
            // agree with the one the triangles give.
            let mut mine: Vec<usize> = (0..ncomp)
                .filter(|&k| !has_contact[k] && demand.per_component_mm2[k] > 0.0)
                .collect();
            mine.sort_unstable();
            let mut theirs = own.components_without_contact.clone();
            theirs.sort_unstable();
            if mine != theirs {
                state_c = ClauseState::Indeterminate;
                indeterminate = true;
                lines.push(indet_line(
                    &file,
                    mi,
                    "component_routing_failed",
                    &[
                        ("reconstructed_starved", mine.len().to_string()),
                        ("partition_starved", theirs.len().to_string()),
                        ("detail", "starved_component_sets_disagree".into()),
                    ],
                ));
            }
        }

        // -- clause (a): unassigned area -----------------------------------
        let state_a = if budget_hit.is_some() {
            ClauseState::Indeterminate
        } else {
            clause_a_unowned(own.a_unowned_mm2, censor)
        };
        let largest_unowned = own.largest_unowned_component.map_or(0.0, |(_, a, _)| a);
        if state_a == ClauseState::Fail {
            failed = true;
            lines.push(Failure::line(
                "LOAD_UNOWNED",
                &file,
                mi,
                None,
                None,
                &[
                    ("a_unowned_mm2", num(own.a_unowned_mm2)),
                    ("censor_mm2", format!("{censor:e}")),
                    (
                        "components_without_contact",
                        own.components_without_contact.len().to_string(),
                    ),
                    ("largest_unowned_component_mm2", num(largest_unowned)),
                ],
                {
                    let mut o = Json::obj();
                    o.set("a_unowned_mm2", Json::num(own.a_unowned_mm2));
                    o.set("censor_mm2", Json::num(censor));
                    o.set(
                        "components_without_contact",
                        Json::usize(own.components_without_contact.len()),
                    );
                    o.set("largest_unowned_component_mm2", Json::num(largest_unowned));
                    o
                },
            ));
        }

        // One line per starved component, worst demand first.
        let mut starved: Vec<usize> = (0..ncomp)
            .filter(|&k| !has_contact[k] && demand.per_component_mm2[k] > 0.0)
            .collect();
        starved.sort_by(|&a, &b| {
            demand.per_component_mm2[b]
                .partial_cmp(&demand.per_component_mm2[a])
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        if !starved.is_empty() {
            failed = true;
        }
        for &k in starved.iter().take(MAX_COMPONENT_LINES) {
            let ctr = centroid_of(k);
            lines.push(Failure::line(
                "LOAD_COMPONENT_UNSUPPORTED",
                &file,
                mi,
                None,
                None,
                &[
                    ("component", k.to_string()),
                    ("area_mm2", num(true_area_by_component[k])),
                    ("demand_mm2", num(demand.per_component_mm2[k])),
                    ("centroid", format!("{},{},{}", num(ctr[0]), num(ctr[1]), num(ctr[2]))),
                ],
                {
                    let mut o = Json::obj();
                    o.set("component", Json::usize(k));
                    o.set("area_mm2", Json::num(true_area_by_component[k]));
                    o.set("demand_mm2", Json::num(demand.per_component_mm2[k]));
                    o.set("centroid_mm", Json::v3(ctr));
                    o
                },
            ));
        }

        // -- clause (d): the engine's books --------------------------------
        //
        // DECLARED, by inversion of A = x_allowed * pi * r_tip^2 (§2.3). It
        // stops being meaningful the moment a future engine sizes tips any
        // other way, which is why the assumption is echoed in `measured`.
        let mut lo = 0.0f64;
        let mut clamped = 0usize;
        let mut unclamped = 0usize;
        let mut disputed_radius: Vec<usize> = Vec::new();
        let mut contact_rows = Json::Arr(Vec::new());
        for (i, s) in ctx.seats.iter().enumerate() {
            let ia = implied_area(s.contact_id, s.r_tip_mm, x_allowed);
            if !s.r_tip_agrees {
                disputed_radius.push(s.contact_id);
            }
            if ia.clamped {
                clamped += 1;
            } else {
                unclamped += 1;
                lo += ia.a_implied_mm2;
            }
            let mut row = Json::obj();
            row.set("contact_id", Json::usize(s.contact_id));
            row.set("r_tip_mm", Json::num(s.r_tip_mm));
            row.set("r_tip_provenance", Json::str("DECLARED"));
            row.set("r_tip_three_way_agrees", Json::Bool(s.r_tip_agrees));
            row.set("r_tip_spread_mm", Json::num(s.r_tip_spread_mm));
            row.set("a_implied_mm2", Json::num(ia.a_implied_mm2));
            row.set("implied_clamped", Json::Bool(ia.clamped));
            row.set(
                "a_owned_mm2",
                Json::num_or_null(own.a_owned_mm2.get(i).copied()),
            );
            row.set(
                "surface_component",
                sites
                    .get(i)
                    .and_then(|s| s.component)
                    .map_or(Json::Null, Json::usize),
            );
            contact_rows.push(row);
        }
        let hi = lo + clamped as f64 * x_allowed * std::f64::consts::PI * r_max * r_max;
        let state_d = if disputed_radius.is_empty() {
            clause_d_books(d_total, lo, hi)
        } else {
            ClauseState::Indeterminate
        };
        if !disputed_radius.is_empty() {
            indeterminate = true;
            lines.push(indet_line(
                &file,
                mi,
                "tip_radius_not_established",
                &[
                    ("contacts", disputed_radius.len().to_string()),
                    ("first_contact", disputed_radius[0].to_string()),
                    (
                        "detail",
                        "the three-way tip-radius comparison of §3.1 disagrees, \
                         so the inversion of §2.3 has no established input"
                            .into(),
                    ),
                ],
            ));
        }
        // **Pre-existing crash, found by the `long_ledge` fixture while
        // verifying the interval sweep, and unrelated to it.** `lo == 0`
        // (no unclamped contact contributes a lower bound) made this
        // `f64::INFINITY`, which `Json::num` asserts against -- the whole
        // audit panicked on the file rather than reporting anything about
        // it. The ratio is genuinely UNDEFINED when the books recover no
        // lower bound, which is what `num_or_null` is for; the guard forbids
        // hiding a NaN produced by arithmetic, not reporting an undefined
        // quotient as absent. Verified against the unmodified binary at
        // c01e9d7 + the new fixtures: it panicked there too.
        let ratio: Option<f64> = (lo > 0.0).then(|| d_total / lo);
        if state_d == ClauseState::Fail {
            failed = true;
            lines.push(Failure::line(
                "LOAD_BOOKS_DISAGREE",
                &file,
                mi,
                None,
                None,
                &[
                    ("d_total_mm2", num(d_total)),
                    ("implied_lo_mm2", num(lo)),
                    ("implied_hi_mm2", num(hi)),
                    ("clamped_contacts", clamped.to_string()),
                    (
                        "ratio",
                        ratio.map_or_else(|| "undefined".to_string(), num),
                    ),
                ],
                {
                    let mut o = Json::obj();
                    o.set("d_total_mm2", Json::num(d_total));
                    o.set("implied_lo_mm2", Json::num(lo));
                    o.set("implied_hi_mm2", Json::num(hi));
                    o.set("clamped_contacts", Json::usize(clamped));
                    o.set("ratio", Json::num_or_null(ratio));
                    o.set("ratio_definition", Json::str("d_total_mm2 / implied_lo_mm2"));
                    o
                },
            ));
        }

        // -- clause (e): D reconstructed a second way, off the raster ------
        let (state_e, rd) = match ctx.model_raster() {
            Ok(r) => {
                let rd = raster_demand(r);
                (
                    clause_e_demand_agrees(d_total, rd.d_raster_mm2, rd.perimeter_total_mm),
                    Some(rd),
                )
            }
            Err(e) => {
                indeterminate = true;
                lines.push(indet_line(
                    &file,
                    mi,
                    "demand_raster_unavailable",
                    &[("detail", format!("{e}"))],
                ));
                (ClauseState::Indeterminate, None)
            }
        };
        let e_bound = rd.map_or(f64::NAN, |rd| {
            0.5 * rd.perimeter_total_mm * c::D2_MM + c::tol_sum(d_total)
        });
        let e_resid = rd.map_or(f64::NAN, |rd| (d_total - rd.d_raster_mm2).abs());
        if state_e == ClauseState::Fail {
            failed = true;
            // §2.1 names this code and this reason for this condition. It is
            // a file-level code raised from a check because the driver's
            // abort path does not compute the double reconstruction.
            lines.push(Failure::line(
                "FILE_MESH_NOT_CLOSED",
                &file,
                mi,
                None,
                None,
                &[
                    ("reason", "demand_reconstruction_disagrees".into()),
                    ("d_total_mm2", num(d_total)),
                    ("d_raster_mm2", num(rd.map_or(f64::NAN, |r| r.d_raster_mm2))),
                    ("residual_mm2", num(e_resid)),
                    ("bound_mm2", num(e_bound)),
                    (
                        "perimeter_total_mm",
                        num(rd.map_or(f64::NAN, |r| r.perimeter_total_mm)),
                    ),
                ],
                {
                    let mut o = Json::obj();
                    o.set("reason", Json::str("demand_reconstruction_disagrees"));
                    o.set("d_total_mm2", Json::num(d_total));
                    o.set(
                        "d_raster_mm2",
                        Json::num_or_null(rd.map(|r| r.d_raster_mm2)),
                    );
                    o.set("residual_mm2", Json::num(e_resid));
                    o.set("bound_mm2", Json::num(e_bound));
                    o
                },
            ));
        }

        // -- the measured block --------------------------------------------
        measured.set("implemented", Json::Bool(true));
        measured.set("d_total_mm2", Json::num(d_total));
        measured.set("d_total_provenance", Json::str("RECONSTRUCTED"));
        measured.set(
            "down_face_true_area_mm2",
            Json::num(demand.down_face_true_area_mm2),
        );
        measured.set(
            "down_facing_triangles",
            Json::usize(demand.down_facing_triangles),
        );
        measured.set("model_triangles", Json::usize(mesh.tri_count()));
        measured.set("surface_components", Json::usize(ncomp));
        measured.set("contacts", Json::usize(sites.len()));
        measured.set("a_owned_total_mm2", Json::num(a_owned_total));
        measured.set("a_unowned_mm2", Json::num(own.a_unowned_mm2));
        measured.set("accounted_mm2", Json::num(own.accounted_mm2));
        measured.set("censor_mm2", Json::num(censor));
        measured.set("samples", Json::num(own.samples as f64));
        measured.set(
            "sample_budget_exceeded",
            budget_hit.map_or(Json::Null, |v| Json::num(v as f64)),
        );
        measured.set("largest_unowned_component_mm2", Json::num(largest_unowned));
        // When the sampler stopped at the budget it stopped MID-MODEL, so
        // a_owned, a_unowned, accounted and every per-component owned total
        // above are partial sums of an unfinished pass. They are published
        // because how far it got is evidence, and flagged because a partial
        // sum read as a total is exactly the kind of quiet lie this instrument
        // exists to find.
        measured.set("partition_partial", Json::Bool(budget_hit.is_some()));
        measured.set(
            "components_without_contact",
            Json::usize(own.components_without_contact.len()),
        );
        measured.set(
            "component_lines_written",
            Json::usize(starved.len().min(MAX_COMPONENT_LINES)),
        );

        measured.set("implied_lo_mm2", Json::num(lo));
        measured.set("implied_hi_mm2", Json::num(hi));
        measured.set("implied_provenance", Json::str("DECLARED_BY_INVERSION"));
        measured.set(
            "implied_assumption",
            Json::str(
                "the engine sizes every tip to exactly X = carried_area/(pi r^2) = x_allowed; \
                 measured 2026-08-30, max deviation 7e-15. A future engine that sizes tips \
                 any other way makes this bracket meaningless and it must be re-derived",
            ),
        );
        measured.set("clamped_contacts", Json::usize(clamped));
        measured.set("unclamped_contacts", Json::usize(unclamped));
        measured.set("tip_radius_disputed_contacts", Json::usize(disputed_radius.len()));
        measured.set("books_ratio", Json::num_or_null(ratio));
        measured.set("books_ratio_definition", Json::str("d_total_mm2 / implied_lo_mm2"));
        measured.set("books_tolerance_mm2", Json::num(c::tol_sum(d_total)));

        measured.set(
            "d_raster_mm2",
            Json::num_or_null(rd.map(|r| r.d_raster_mm2)),
        );
        measured.set(
            "d_raster_perimeter_total_mm",
            Json::num_or_null(rd.map(|r| r.perimeter_total_mm)),
        );
        measured.set(
            "d_raster_layers_with_down_face",
            rd.map_or(Json::Null, |r| Json::usize(r.layers_with_down_face)),
        );
        measured.set(
            "d_raster_down_face_pixels",
            rd.map_or(Json::Null, |r| Json::num(r.down_face_pixels as f64)),
        );
        measured.set("d_raster_residual_mm2", Json::num(e_resid));
        measured.set("d_raster_bound_mm2", Json::num(e_bound));

        measured.set("b_literal_residual_mm2", Json::num(b_literal_residual));
        measured.set("b_literal_state", Json::str(b_literal_state.as_str()));
        measured.set(
            "b_literal_note",
            Json::str(
                "clause (b) read literally compares sum A_owned against D_total; by §2.2 that \
                 difference IS A_unowned, so the literal clause restates clause (a) at a far \
                 tighter tolerance. Graded here on accounted = sum A_owned + A_unowned, which \
                 is the partition identity its own prose describes. No strictness is lost: any \
                 A_unowned > 0 raises LOAD_COMPONENT_UNSUPPORTED at zero censor",
            ),
        );

        let mut clauses = Json::obj();
        clauses.set("a_unowned", Json::str(state_a.as_str()));
        clauses.set("b_partition_identity", Json::str(state_b.as_str()));
        clauses.set("c_component_routing", Json::str(state_c.as_str()));
        clauses.set("d_books_bracket", Json::str(state_d.as_str()));
        clauses.set("e_demand_double_reconstruction", Json::str(state_e.as_str()));
        measured.set("clauses", clauses);

        let mut comp_rows = Json::Arr(Vec::new());
        let mut order: Vec<usize> = (0..ncomp).collect();
        order.sort_by(|&a, &b| {
            demand.per_component_mm2[b]
                .partial_cmp(&demand.per_component_mm2[a])
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for &k in order.iter().take(MAX_COMPONENT_ROWS) {
            let mut row = Json::obj();
            row.set("component", Json::usize(k));
            row.set("demand_mm2", Json::num(demand.per_component_mm2[k]));
            row.set("down_face_true_area_mm2", Json::num(true_area_by_component[k]));
            row.set("owned_mm2", Json::num(owned_by_component[k]));
            row.set("contacts", Json::usize(contacts_by_component[k]));
            row.set("centroid_mm", Json::v3(centroid_of(k)));
            comp_rows.push(row);
        }
        measured.set("components", comp_rows);
        measured.set("components_listed", Json::usize(ncomp.min(MAX_COMPONENT_ROWS)));
        measured.set("contact_rows", contact_rows);

        measured.set(
            "headline",
            Json::str(headline(
                d_total,
                lo,
                hi,
                own.a_unowned_mm2,
                starved.len(),
                indeterminate,
                budget_hit.is_some(),
            )),
        );

        // -- counting: one item, and indeterminate dominates ----------------
        let mut outcome = CheckOutcome::new(1);
        outcome.measured = measured;
        outcome.failures = lines;
        if indeterminate {
            outcome.indeterminate = 1;
        } else if failed {
            outcome.failed = 1;
        }
        outcome
    }
}

/// The short string the human summary prints after the counts.
fn headline(
    d_total: f64,
    lo: f64,
    hi: f64,
    a_unowned: f64,
    starved: usize,
    indeterminate: bool,
    partial: bool,
) -> String {
    let pct = if d_total > 0.0 {
        100.0 * a_unowned / d_total
    } else {
        0.0
    };
    let mut s = format!(
        "books [{}, {}] mm2, demand {} mm2",
        num(lo),
        num(hi),
        num(d_total)
    );
    if partial {
        // Reporting an unowned total off a truncated pass would be a number
        // the audit did not measure.
        s.push_str("; ownership NOT MEASURED, sampler stopped at its budget");
    } else {
        s.push_str(&format!("; unowned {} mm2 ({pct:.1} %)", num(a_unowned)));
    }
    if starved > 0 {
        s.push_str(&format!(", {starved} starved components"));
    }
    if indeterminate {
        s.push_str(" -- INDETERMINATE");
    }
    s
}

/// `LOAD_OWNERSHIP_INDETERMINATE`, the one shape every unestablished result
/// takes. "Not measured" and "indeterminate" never map to pass.
fn indet_line(file: &str, model: usize, reason: &str, extra: &[(&str, String)]) -> Failure {
    let mut fields: Vec<(&str, String)> = vec![("reason", reason.to_string())];
    fields.extend(extra.iter().map(|(k, v)| (*k, v.clone())));
    let mut values = Json::obj();
    values.set("reason", Json::str(reason));
    for (k, v) in extra {
        values.set(k, Json::str(v.clone()));
    }
    Failure::line(
        "LOAD_OWNERSHIP_INDETERMINATE",
        file,
        model,
        None,
        None,
        &fields,
        values,
    )
}

/// One number formatter, so a line and its JSON row never drift.
fn num(v: f64) -> String {
    if v.is_nan() {
        "nan".to_string()
    } else if v.is_infinite() {
        if v > 0.0 { "inf" } else { "-inf" }.to_string()
    } else {
        format!("{v}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raster::{Layer, RasterGrid, Run, RunRaster};
    use std::collections::BTreeMap;

    // -- clause (a) -----------------------------------------------------

    /// **This test asserted `clause_a_unowned(px, px) == Pass` before
    /// `audit-interval-policy.md` §3.3 row 11** -- unowned area sitting
    /// exactly ON the censor was certified as under it. Clause (a) is a
    /// MAXIMUM-type relation, so the conservative bound of `A_unowned` is
    /// its UPPER bound and the censor must be cleared with room.
    ///
    /// Note the direction: this is the OPPOSITE bound from clause (d)'s low
    /// end and from I1/I2/I3's minimum-type rows, which is why the sweep
    /// could not be applied mechanically.
    #[test]
    fn clause_a_refuses_unowned_area_it_cannot_place_under_the_censor() {
        let px = c::PIXEL_AREA_MM2;
        // Genuinely nothing unowned: `tol_sum(0)` is 1e-9 mm2 against a
        // 2.66e-4 mm2 censor, so this still clears with five decades to
        // spare. The gate is not vacuous in the passing direction.
        assert_eq!(clause_a_unowned(0.0, px), ClauseState::Pass);
        assert_eq!(clause_a_unowned(px * 0.5, px), ClauseState::Pass);
        // Exactly at the censor: no longer a pass. The evidence cannot place
        // it under the limit.
        assert_eq!(
            clause_a_unowned(px, px),
            ClauseState::Indeterminate,
            "at the censor is not established to be under it"
        );
        // One ULP above is likewise unresolved, not yet a measured breach --
        // it is well inside `tol_sum`. Both gate as failures regardless.
        let just_over = f64::from_bits(px.to_bits() + 1);
        assert_eq!(
            clause_a_unowned(just_over, px),
            ClauseState::Indeterminate
        );
        assert!(!clause_a_unowned(just_over, px).is_pass());
        // Clear of the band: a measured breach, still distinguishable.
        assert_eq!(clause_a_unowned(px * 2.0, px), ClauseState::Fail);
        // The real corpus scale: hundreds of mm2 unassigned.
        assert_eq!(clause_a_unowned(310.0, px), ClauseState::Fail);
    }

    #[test]
    fn an_unmeasurable_unowned_area_is_indeterminate_and_never_a_pass() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                clause_a_unowned(bad, c::PIXEL_AREA_MM2),
                ClauseState::Indeterminate,
                "{bad} must not pass"
            );
            assert!(!clause_a_unowned(bad, c::PIXEL_AREA_MM2).is_pass());
        }
        assert_eq!(
            clause_a_unowned(0.0, f64::NAN),
            ClauseState::Indeterminate,
            "an unmeasurable censor is unmeasurable too"
        );
    }

    // -- clause (b) -----------------------------------------------------

    #[test]
    fn clause_b_holds_at_the_summation_bound_and_breaks_beyond_it() {
        let d = 500.0;
        let tol = c::tol_sum(d);
        assert_eq!(clause_b_identity(d, d), ClauseState::Pass);
        assert_eq!(clause_b_identity(d + tol, d), ClauseState::Pass);
        assert_eq!(clause_b_identity(d - tol, d), ClauseState::Pass);
        assert_eq!(clause_b_identity(d + tol * 2.0, d), ClauseState::Fail);
        // The absolute leg dominates at zero demand.
        assert_eq!(clause_b_identity(1e-10, 0.0), ClauseState::Pass);
        assert_eq!(clause_b_identity(1e-8, 0.0), ClauseState::Fail);
    }

    #[test]
    fn clause_b_on_an_unmeasurable_input_is_indeterminate() {
        assert_eq!(
            clause_b_identity(f64::NAN, 100.0),
            ClauseState::Indeterminate
        );
        assert_eq!(
            clause_b_identity(100.0, f64::NAN),
            ClauseState::Indeterminate
        );
    }

    // -- clause (c) -----------------------------------------------------

    #[test]
    fn clause_c_passes_a_clean_partition_and_catches_a_cross_component_leak() {
        let demand = [100.0, 40.0, 0.0];
        let has = [true, true, false];
        assert_eq!(
            clause_c_components(&[100.0, 40.0, 0.0], &demand, &has).0,
            ClauseState::Pass
        );
        // Component 0 owns 5 mm2 that belongs to component 1.
        let (state, worst) = clause_c_components(&[105.0, 35.0, 0.0], &demand, &has);
        assert_eq!(state, ClauseState::Indeterminate);
        let (k, resid, _) = worst.expect("the leak is named");
        assert!(k == 0 || k == 1);
        assert!((resid - 5.0).abs() < 1e-9, "{resid}");
    }

    #[test]
    fn clause_c_requires_a_starved_component_to_own_nothing() {
        // Component 2 has no contact, so anything owned there is a routing
        // defect and not merely unassigned area.
        let (state, _) = clause_c_components(&[100.0, 0.0, 7.0], &[100.0, 0.0, 7.0], &[
            true, false, false,
        ]);
        assert_eq!(state, ClauseState::Indeterminate);
    }

    #[test]
    fn clause_c_on_unmeasurable_or_mismatched_input_is_indeterminate() {
        assert_eq!(
            clause_c_components(&[f64::NAN], &[1.0], &[true]).0,
            ClauseState::Indeterminate
        );
        assert_eq!(
            clause_c_components(&[1.0, 2.0], &[1.0], &[true]).0,
            ClauseState::Indeterminate,
            "a length mismatch is an instrument fault, not a pass"
        );
    }

    // -- clause (d) -----------------------------------------------------

    /// **This test asserted `clause_d_books(lo - tol, lo, hi) == Pass`
    /// before `audit-interval-policy.md` §3.3 row 14** -- a reconstructed
    /// demand genuinely BELOW the books' own lower end, certified because
    /// the shortfall was inside `tol_sum`. Row 14 is the widest of the ten
    /// optimistic comparisons because it did that at both ends at once.
    #[test]
    fn clause_d_refuses_a_demand_it_cannot_place_inside_the_bracket() {
        // A genuinely wide bracket: 1 clamped contact, so the books admit an
        // interval rather than a point. This is the shape the clause can
        // actually certify -- see the zero-width case below.
        let lo = 10.0;
        let hi = 120.0;
        let d = 100.0;
        let tol = c::tol_sum(d);
        assert_eq!(clause_d_books(d, lo, hi), ClauseState::Pass);

        // The retired form's own boundary cases, both ends. Both passed;
        // neither may.
        assert_eq!(
            clause_d_books(lo - tol * 0.5, lo, hi),
            ClauseState::Indeterminate,
            "below the books' low end used to certify"
        );
        assert_eq!(
            clause_d_books(hi + c::tol_sum(hi) * 0.5, lo, hi),
            ClauseState::Indeterminate,
            "above the books' high end used to certify"
        );
        // Clear of the bracket at either end is still a measured defect.
        assert_eq!(clause_d_books(lo * 0.5, lo, hi), ClauseState::Fail);
        assert_eq!(clause_d_books(hi * 3.0, lo, hi), ClauseState::Fail);
    }

    /// **A zero-width bracket can no longer PASS, and that is a real
    /// consequence worth pinning rather than discovering later.** With no
    /// clamped contacts the books recover a point (`lo == hi`), and a point
    /// cannot contain a nonzero-width interval: §3.3's rule needs
    /// `D - tol >= lo` AND `D + tol <= hi` simultaneously, which no finite
    /// `tol > 0` satisfies when `lo == hi`.
    ///
    /// Measured on the artifacts in this checkout, this changes nothing:
    /// every point-bracket file already failed clause (d) under the retired
    /// form, with `|D - lo|` four to six decades ABOVE `tol_sum(D)`
    /// (`bare_island` 3.7e-7 against 5.8e-12; `collinear_arms` 6.4e-6
    /// against 9.3e-11; `starved_patch` 3.3e-8 against 1.2e-12). The
    /// tolerance was never doing work at a point bracket.
    #[test]
    fn a_zero_width_bracket_can_never_certify_and_the_shortfall_is_still_measured() {
        let a = 12.0 * std::f64::consts::PI * 0.197 * 0.197;
        let lo = 24.0 * a;
        let hi = lo; // no clamped contacts, so the bracket is a point
        assert_eq!(
            clause_d_books(lo, lo, hi),
            ClauseState::Indeterminate,
            "a point bracket cannot contain an interval of nonzero width"
        );
        // And a demand clear of the point is still a measured FAIL, so the
        // clause has not collapsed into reporting one value for everything.
        assert_eq!(clause_d_books(lo * 3.0, lo, hi), ClauseState::Fail);
        assert_eq!(clause_d_books(lo * 0.1, lo, hi), ClauseState::Fail);
    }

    #[test]
    fn a_clamped_tip_widens_the_bracket_instead_of_being_exempted() {
        let r_max = c::TIP_RADIUS_MAX_MM;
        let lo = 10.0;
        let one_clamped = 12.0 * std::f64::consts::PI * r_max * r_max;
        let hi = lo + one_clamped;
        // Demand inside the interval passes; beyond its top it still fails,
        // so the interval refuses rather than excusing.
        assert_eq!(clause_d_books(lo + 1.0, lo, hi), ClauseState::Pass);
        assert_eq!(clause_d_books(hi * 2.0, lo, hi), ClauseState::Fail);
        assert_eq!(clause_d_books(lo - 1.0, lo, hi), ClauseState::Fail);
    }

    #[test]
    fn clause_d_refuses_a_malformed_or_unmeasurable_bracket() {
        assert_eq!(clause_d_books(1.0, 5.0, 2.0), ClauseState::Indeterminate);
        assert_eq!(
            clause_d_books(f64::NAN, 0.0, 1.0),
            ClauseState::Indeterminate
        );
        assert_eq!(
            clause_d_books(1.0, f64::NAN, 1.0),
            ClauseState::Indeterminate
        );
    }

    // -- clause (e) -----------------------------------------------------

    #[test]
    fn clause_e_allows_exactly_half_a_pixel_diagonal_along_the_boundary() {
        let d = 100.0;
        let perim = 400.0;
        let bound = 0.5 * perim * c::D2_MM + c::tol_sum(d);
        assert_eq!(
            clause_e_demand_agrees(d, d - bound, perim),
            ClauseState::Pass
        );
        assert_eq!(
            clause_e_demand_agrees(d, d - bound * 1.0001, perim),
            ClauseState::Fail
        );
        // With no boundary at all the bound collapses to the summation bound.
        assert_eq!(clause_e_demand_agrees(d, d, 0.0), ClauseState::Pass);
        assert_eq!(clause_e_demand_agrees(d, d - 0.001, 0.0), ClauseState::Fail);
    }

    #[test]
    fn clause_e_on_an_absent_second_reconstruction_is_indeterminate() {
        assert_eq!(
            clause_e_demand_agrees(100.0, f64::NAN, 10.0),
            ClauseState::Indeterminate,
            "no second reconstruction means the property is not established"
        );
        assert_eq!(
            clause_e_demand_agrees(100.0, 100.0, -1.0),
            ClauseState::Indeterminate
        );
    }

    // -- the run algebra and the raster reconstruction -------------------

    #[test]
    fn run_subtraction_is_exact_on_overlap_containment_and_disjointness() {
        let a = [Run { c0: 0, c1: 10 }];
        assert_eq!(subtract_runs(&a, &[]), vec![Run { c0: 0, c1: 10 }]);
        assert_eq!(subtract_runs(&a, &[Run { c0: 0, c1: 10 }]), vec![]);
        assert_eq!(
            subtract_runs(&a, &[Run { c0: 3, c1: 5 }]),
            vec![Run { c0: 0, c1: 3 }, Run { c0: 5, c1: 10 }]
        );
        assert_eq!(
            subtract_runs(&a, &[Run { c0: -5, c1: 4 }, Run { c0: 8, c1: 20 }]),
            vec![Run { c0: 4, c1: 8 }]
        );
        assert_eq!(
            subtract_runs(&a, &[Run { c0: 20, c1: 30 }]),
            vec![Run { c0: 0, c1: 10 }]
        );
    }

    fn grid() -> RasterGrid {
        RasterGrid {
            pitch_x_mm: c::PIXEL_PITCH_X_MM,
            pitch_y_mm: c::PIXEL_PITCH_Y_MM,
            layer_height_mm: c::LAYER_HEIGHT_DEFAULT_MM,
            z_plate_mm: 0.0,
        }
    }

    fn layer(k: i64, rows: &[(i64, &[Run])]) -> Layer {
        let mut map: BTreeMap<i64, Vec<Run>> = BTreeMap::new();
        for (r, runs) in rows {
            map.insert(*r, runs.to_vec());
        }
        Layer {
            k,
            z_mm: k as f64 * c::LAYER_HEIGHT_DEFAULT_MM,
            rows: map,
        }
    }

    #[test]
    fn a_straight_column_presents_its_footprint_once_and_only_once() {
        // A 10 x 4 pixel prism, three layers tall. Only the bottom layer is
        // down-facing, so D_raster is one footprint, not three.
        let runs = [Run { c0: 0, c1: 10 }];
        let rows: Vec<(i64, &[Run])> = (0..4).map(|j| (j, &runs[..])).collect();
        let r = RunRaster {
            grid: grid(),
            layers: (0..3).map(|k| layer(k, &rows)).collect(),
            pixel_count: 120,
            run_count: 12,
        };
        let rd = raster_demand(&r);
        assert_eq!(rd.down_face_pixels, 40);
        let want = 40.0 * c::PIXEL_PITCH_X_MM * c::PIXEL_PITCH_Y_MM;
        assert!((rd.d_raster_mm2 - want).abs() < 1e-15, "{}", rd.d_raster_mm2);
        assert_eq!(rd.layers_with_down_face, 1);
        // Perimeter of a 10 x 4 rectangle of pixels.
        let want_p = 2.0 * (10.0 * c::PIXEL_PITCH_X_MM) + 2.0 * (4.0 * c::PIXEL_PITCH_Y_MM);
        assert!(
            (rd.perimeter_total_mm - want_p).abs() < 1e-12,
            "{}",
            rd.perimeter_total_mm
        );
    }

    #[test]
    fn a_ledge_adds_its_own_down_face_on_the_layer_it_appears() {
        // Layer 0: columns 0..4. Layer 1: columns 0..10, so 6 new columns are
        // down-facing overhang.
        let narrow = [Run { c0: 0, c1: 4 }];
        let wide = [Run { c0: 0, c1: 10 }];
        let r = RunRaster {
            grid: grid(),
            layers: vec![
                layer(0, &[(0, &narrow[..])]),
                layer(1, &[(0, &wide[..])]),
            ],
            pixel_count: 14,
            run_count: 2,
        };
        let rd = raster_demand(&r);
        assert_eq!(rd.down_face_pixels, 4 + 6);
        assert_eq!(rd.layers_with_down_face, 2);
    }

    #[test]
    fn a_gap_in_the_layer_stack_makes_the_whole_layer_down_facing() {
        // Layers 0 and 2 are cured, layer 1 is not: the cavity ceiling at k=2
        // is a complete down-face, not a difference against k=0.
        let runs = [Run { c0: 0, c1: 5 }];
        let r = RunRaster {
            grid: grid(),
            layers: vec![layer(0, &[(0, &runs[..])]), layer(2, &[(0, &runs[..])])],
            pixel_count: 10,
            run_count: 2,
        };
        let rd = raster_demand(&r);
        assert_eq!(
            rd.down_face_pixels, 10,
            "an omitted layer is void, so the layer above it is all ceiling"
        );
    }

    #[test]
    fn an_empty_raster_reconstructs_no_demand_rather_than_refusing() {
        let rd = raster_demand(&RunRaster::empty(grid()));
        assert_eq!(rd.d_raster_mm2, 0.0);
        assert_eq!(rd.perimeter_total_mm, 0.0);
        // And the clause then compares 0 against 0, which is a real answer.
        assert_eq!(clause_e_demand_agrees(0.0, 0.0, 0.0), ClauseState::Pass);
    }

    // -- the failure lines ------------------------------------------------

    #[test]
    fn the_censor_prints_as_the_specification_spells_it() {
        assert_eq!(format!("{:e}", c::PIXEL_AREA_MM2), "2.66e-4");
    }

    // -- end to end, on a solid whose answer is known analytically ---------
    //
    // A predicate that is only ever tested on hand-fed numbers proves nothing
    // about whether the check FIRES. These two run the real `Check::run` over
    // a real container, through the real reader, welder, seat reconstruction,
    // Voronoi partition and rasteriser, on a 2 x 2 mm slab whose down-facing
    // demand is exactly 4 mm^2 by hand.

    use crate::context::{AuditContext, AuditOptions};
    use crate::reader::{ChituboxFile, RecordKind as RK};
    use crate::synth::{box_soup, SynthFile, SynthModel, SynthRecord};

    /// A 2 x 2 x 1 mm slab with `n` tips of radius `r_tip` seated in it, run
    /// through the whole audit path. Returns I4's outcome.
    fn slab_outcome(tag: &str, tips: &[f64]) -> CheckOutcome {
        let f = c::BALL_FACTOR_STUB;
        let mut records: Vec<SynthRecord> = Vec::new();
        let grid_xy = [-0.75f64, -0.25, 0.25, 0.75];
        let mut i = 0usize;
        'outer: for &x in &grid_xy {
            for &y in &grid_xy {
                let Some(&r_tip) = tips.get(i) else {
                    break 'outer;
                };
                i += 1;
                let r_ball = f * r_tip;
                // p0 is the ball's south pole and must lie INSIDE the solid,
                // or the seat has no entry triangle and nothing routes.
                let p0 = [x, y, -0.35];
                let centre = [x, y, -0.35 + r_ball];
                let end = [x, y, -0.35 + 2.0 * r_ball];
                records.push(
                    SynthRecord::new(RK::ContactSphere, p0, end, r_ball, r_ball)
                        .with_matching_soup(),
                );
                records.push(
                    SynthRecord::new(
                        RK::ContactTaper,
                        centre,
                        [x, y, centre[2] + 0.5],
                        r_tip,
                        r_tip,
                    )
                    .with_declared_depth(0.3),
                );
            }
        }
        let model = SynthModel {
            name: "slab".to_string(),
            translate_mm: [0.0, 0.0, 3.0],
            mesh: box_soup([-1.0, -1.0, -0.5], [1.0, 1.0, 0.5]),
            records,
        };
        let path = std::env::temp_dir().join(format!(
            "plinth-audit-i4-{tag}-{}.chitubox",
            std::process::id()
        ));
        SynthFile::new()
            .with_model(model)
            .write(&path)
            .expect("write the synthetic container");
        let file = ChituboxFile::open(&path).expect("the synthetic container parses");
        let options = AuditOptions::default();
        let audit = AuditContext {
            file: &file,
            options: &options,
            sidecar: None,
            sidecar_absence: Some("synthetic fixture".to_string()),
        };
        let ctx = ModelContext::build(&audit, 0);
        let outcome = LoadOwnership.run(&ctx);
        let _ = std::fs::remove_file(&path);
        outcome
    }

    fn measured_num(o: &CheckOutcome, key: &str) -> f64 {
        o.measured
            .get(key)
            .and_then(Json::as_f64)
            .unwrap_or_else(|| panic!("measured.{key} is missing"))
    }
    fn clause(o: &CheckOutcome, key: &str) -> String {
        o.measured
            .get("clauses")
            .and_then(|c| c.get(key))
            .and_then(Json::as_str)
            .unwrap_or("<missing>")
            .to_string()
    }

    #[test]
    fn end_to_end_the_books_clause_fires_when_the_tips_are_sized_for_less_load_than_exists() {
        // 16 tips at r = 0.06 mm book 16 * 12 * pi * 0.0036 = 2.1715 mm^2
        // against a 4 mm^2 underside: the corpus's own direction of error.
        let o = slab_outcome("under", &[0.06; 16]);
        assert!(!o.is_not_implemented(), "the check must actually run");
        assert!(
            (measured_num(&o, "d_total_mm2") - 4.0).abs() < 1e-12,
            "a 2 x 2 mm underside is 4 mm^2 of demand, measured {}",
            measured_num(&o, "d_total_mm2")
        );
        assert_eq!(measured_num(&o, "contacts") as usize, 16);
        assert_eq!(clause(&o, "d_books_bracket"), "fail");
        let lo = measured_num(&o, "implied_lo_mm2");
        let want = 16.0 * 12.0 * std::f64::consts::PI * 0.06 * 0.06;
        assert!((lo - want).abs() < 1e-4, "lo {lo} against {want}");
        assert!(
            (measured_num(&o, "books_ratio") - 4.0 / want).abs() < 1e-4,
            "the ratio says by how much"
        );
        assert!(
            o.failures
                .iter()
                .any(|f| f.code == "LOAD_BOOKS_DISAGREE" && f.message.contains("ratio=")),
            "the failure must say by how much: {:?}",
            o.failures.iter().map(|f| f.code).collect::<Vec<_>>()
        );
        // Nothing is unassigned on a slab whose one component carries tips.
        assert_eq!(clause(&o, "a_unowned"), "pass");
        assert_eq!(measured_num(&o, "a_unowned_mm2"), 0.0);
    }

    #[test]
    fn end_to_end_a_clamped_tip_widens_the_bracket_and_the_books_clause_passes() {
        // Fifteen unclamped tips book 2.036 mm^2 and one clamp-limited tip
        // enters as an interval up to 12 * pi * 1.0^2, so 4 mm^2 of demand
        // sits inside the bracket. This is the NEGATIVE CONTROL: a check that
        // fails everything looks perfect.
        let mut tips = vec![0.06f64; 15];
        tips.push(c::TIP_RADIUS_MIN_ALLOWED_MM);
        let o = slab_outcome("clamped", &tips);
        assert!(!o.is_not_implemented());
        assert_eq!(measured_num(&o, "clamped_contacts") as usize, 1);
        assert_eq!(measured_num(&o, "unclamped_contacts") as usize, 15);
        assert_eq!(
            clause(&o, "d_books_bracket"),
            "pass",
            "lo {} hi {} demand {}",
            measured_num(&o, "implied_lo_mm2"),
            measured_num(&o, "implied_hi_mm2"),
            measured_num(&o, "d_total_mm2")
        );
        assert!(
            !o.failures.iter().any(|f| f.code == "LOAD_BOOKS_DISAGREE"),
            "no books line when the bracket holds"
        );
        // The second reconstruction of D ran and produced a number.
        assert!(measured_num(&o, "d_raster_mm2") > 0.0);
    }

    #[test]
    fn an_indeterminate_line_names_its_reason_in_both_the_text_and_the_json() {
        let f = indet_line("/tmp/a.chitubox", 0, "sample_budget_exceeded", &[
            ("samples", "40000001".into()),
        ]);
        assert_eq!(f.code, "LOAD_OWNERSHIP_INDETERMINATE");
        assert!(
            f.message.contains("reason=sample_budget_exceeded"),
            "{}",
            f.message
        );
        assert_eq!(
            f.to_json()
                .get("values")
                .and_then(|v| v.get("reason"))
                .and_then(Json::as_str),
            Some("sample_budget_exceeded")
        );
    }
}
