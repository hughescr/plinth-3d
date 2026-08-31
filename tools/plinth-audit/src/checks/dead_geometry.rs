//! I8 no dead geometry -- `docs/specs/audit-invariants.md` §3.8.
//!
//! Two independent predicates, both gating: `degenerate_count == 0` and
//! `dead_count == 0`.
//!
//! **Degeneracy** is exact, no tolerance: a field-level defect
//! ([`GeomError`] -- zero-length axis, non-positive radius, non-finite field)
//! is `GEOM_DEGENERATE`; an empty soup, a zero-area triangle or a non-finite
//! soup vertex is also `GEOM_DEGENERATE`; a soup that does not lie on its own
//! record's analytic surface within `tol_geom` is `GEOM_MESH_DISAGREES_WITH_RECORD`.
//! A record failing any of these has no solid worth reasoning about further,
//! so it does not also receive a liveness verdict.
//!
//! **Liveness** is by structural role, never by load alone, and is evaluated
//! only for records that passed every degeneracy test:
//!
//! * R1 -- a TIP with `A_owned(c) > pixel_area`: it carries demand.
//! * R2 -- removing it disconnects at least one live TIP from ground (a
//!   single-removal test on [`crate::graph::SupportGraph::ground_reachable_without`]).
//! * R3 -- removing it turns a neighbouring degree-3 non-grounded junction
//!   into a degree-2 pass-through, merging the two spans that met there into
//!   one longer free span. This is the brace/gusset role.
//! * R4 -- it is a raft primitive and shares a connected component with a
//!   live TIP: something live is grounded through it.
//!
//! A primitive with none of R1-R4 is dead. A TIP is graded ONLY by R1/R2/R3
//! applied to itself (R4 never applies to a TIP) -- it is dead even when its
//! parent member is live, because roles are per-primitive and never inherited
//! downward.
//!
//! **Never established never means pass.** If the ownership partition hit its
//! sample budget, R1 cannot be established for any TIP, and every record in
//! the model is reported `GEOM_INDETERMINATE reason=ownership_indeterminate`
//! rather than graded on a guess. A record touched by an unresolved graph
//! ambiguity (anything but a bare unjoined point) is reported
//! `GEOM_INDETERMINATE reason=path_indeterminate` and excluded from role
//! evaluation, because its true connectivity is not established either.
//!
//! ## Known simplifications, disclosed rather than hidden
//!
//! * Ground clause (ii) here is `h_delivered > 0 and h_delivered >= h_required`
//!   (I1's own predicate, read from the shared [`crate::seat::Seat`]
//!   reconstruction). The full specification also requires the seat zone to
//!   pass I6; that check lives in `intersection.rs`, is not owned by this
//!   file, and re-deriving it here would grade a different solid from I6
//!   itself. A tip whose I6 seat zone would fail but whose I1 depth passes is
//!   therefore treated as grounded here -- an optimistic bias, disclosed.
//! * R3's "free span" is the total centreline length of the maximal run of
//!   degree-2, non-grounded records between two junction nodes, where a
//!   junction node is grounded or has degree != 2. Removing a candidate
//!   record that is one of exactly three edges at a non-grounded node merges
//!   the two spans meeting there, which is credited as R3 without separately
//!   re-verifying that a specific flanking member is independently live. This
//!   over-credits liveness in the rare case where both flanking runs are
//!   themselves already dead, which biases toward NOT flagging a brace as
//!   dead rather than toward wrongly flagging one -- see "smallest defect"
//!   below.
//!
//! ## The failure codes this check emits
//!
//! `GEOM_DEGENERATE`, `GEOM_MESH_DISAGREES_WITH_RECORD`, `GEOM_DEAD_MEMBER`,
//! `GEOM_DEAD_TIP`, `GEOM_INDETERMINATE` (reason `ownership_indeterminate` or
//! `path_indeterminate`).

use crate::check::{Check, CheckOutcome, Failure};
use crate::consts as c;
use crate::context::ModelContext;
use crate::graph::{Ambiguity, Link};
use crate::json::Json;
use crate::primitives::GeomError;
use crate::reader::{RecordKind, SupportRecord};
use std::collections::{HashMap, HashSet};

pub struct DeadGeometry;

impl Check for DeadGeometry {
    fn id(&self) -> &'static str {
        "i8_no_dead_geometry"
    }

    fn title(&self) -> &'static str {
        "I8 no dead geometry"
    }

    fn scope(&self, ctx: &ModelContext) -> usize {
        ctx.model.records.len()
    }

    #[allow(clippy::too_many_lines)]
    fn run(&self, ctx: &ModelContext) -> CheckOutcome {
        let file = ctx.file();
        let n = ctx.model.records.len();
        let mut outcome = CheckOutcome::new(n);

        // -------------------------------------------------------------
        // Pass 1: degeneracy. Exact tests, no tolerance except the one
        // §1.4 mesh-agreement check, which already carries its own
        // magnitude-derived tolerance.
        // -------------------------------------------------------------
        let mut degenerate_count = 0usize;
        let mut degenerate_reasons: HashMap<usize, Vec<&'static str>> = HashMap::new();
        let mut mesh_agrees: HashMap<usize, bool> = HashMap::new();

        for i in 0..n {
            let rec = &ctx.model.records[i];
            let kind_label = rec.kind.label();

            match &ctx.primitives[i] {
                Err(e) => {
                    let e = *e;
                    degenerate_count += 1;
                    degenerate_reasons.entry(i).or_default().push(e.reason());
                    let value = degenerate_value(e, rec);
                    outcome.fail(Failure::line(
                        "GEOM_DEGENERATE",
                        &file,
                        ctx.model_index,
                        None,
                        Some(i),
                        &[
                            ("type", kind_label.to_string()),
                            ("reason", e.reason().to_string()),
                            ("value", format!("{value}")),
                        ],
                        {
                            let mut o = Json::obj();
                            o.set("type", Json::str(kind_label));
                            o.set("reason", Json::str(e.reason()));
                            o.set("value", value_json(value));
                            o
                        },
                    ));
                    continue;
                }
                Ok(p) => {
                    let soup = &rec.soup;
                    if soup.is_empty() {
                        degenerate_count += 1;
                        degenerate_reasons.entry(i).or_default().push("empty_soup");
                        outcome.fail(Failure::line(
                            "GEOM_DEGENERATE",
                            &file,
                            ctx.model_index,
                            None,
                            Some(i),
                            &[
                                ("type", kind_label.to_string()),
                                ("reason", "empty_soup".to_string()),
                                ("value", "0".to_string()),
                            ],
                            {
                                let mut o = Json::obj();
                                o.set("type", Json::str(kind_label));
                                o.set("reason", Json::str("empty_soup"));
                                o.set("value", Json::usize(0));
                                o
                            },
                        ));
                        continue;
                    }
                    let zero_area = soup.zero_area_triangles();
                    if zero_area > 0 {
                        degenerate_count += 1;
                        degenerate_reasons
                            .entry(i)
                            .or_default()
                            .push("zero_area_triangle");
                        outcome.fail(Failure::line(
                            "GEOM_DEGENERATE",
                            &file,
                            ctx.model_index,
                            None,
                            Some(i),
                            &[
                                ("type", kind_label.to_string()),
                                ("reason", "zero_area_triangle".to_string()),
                                ("value", zero_area.to_string()),
                            ],
                            {
                                let mut o = Json::obj();
                                o.set("type", Json::str(kind_label));
                                o.set("reason", Json::str("zero_area_triangle"));
                                o.set("value", Json::usize(zero_area));
                                o
                            },
                        ));
                    }
                    if soup.has_nonfinite() {
                        degenerate_count += 1;
                        degenerate_reasons.entry(i).or_default().push("nonfinite");
                        outcome.fail(Failure::line(
                            "GEOM_DEGENERATE",
                            &file,
                            ctx.model_index,
                            None,
                            Some(i),
                            &[
                                ("type", kind_label.to_string()),
                                ("reason", "nonfinite".to_string()),
                                ("value", "soup".to_string()),
                            ],
                            {
                                let mut o = Json::obj();
                                o.set("type", Json::str(kind_label));
                                o.set("reason", Json::str("nonfinite"));
                                o.set("value", Json::str("soup"));
                                o
                            },
                        ));
                    }
                    let agreement = p.verify_against_soup(rec);
                    mesh_agrees.insert(i, agreement.agrees);
                    if !agreement.agrees {
                        degenerate_count += 1;
                        degenerate_reasons
                            .entry(i)
                            .or_default()
                            .push("mesh_disagreement");
                        let dev = agreement
                            .max_surface_deviation_mm
                            .max(agreement.bbox_overflow_mm);
                        outcome.fail(Failure::line(
                            "GEOM_MESH_DISAGREES_WITH_RECORD",
                            &file,
                            ctx.model_index,
                            None,
                            Some(i),
                            &[
                                ("type", kind_label.to_string()),
                                ("max_deviation_mm", format!("{dev}")),
                                ("tolerance_mm", format!("{}", agreement.tolerance_mm)),
                            ],
                            {
                                let mut o = Json::obj();
                                o.set("type", Json::str(kind_label));
                                o.set("max_deviation_mm", Json::num(dev));
                                o.set("tolerance_mm", Json::num(agreement.tolerance_mm));
                                o
                            },
                        ));
                    }
                }
            }
        }

        let clean_records: Vec<usize> = (0..n)
            .filter(|i| ctx.primitives[*i].is_ok() && !degenerate_reasons.contains_key(i))
            .collect();

        // -------------------------------------------------------------
        // Pass 2: liveness, only over records with no degeneracy finding.
        // -------------------------------------------------------------
        let ownership_budget_exceeded = ctx.ownership().budget_exceeded.is_some();

        let layer = ctx.inputs().layer_height_mm;
        let z_ground = ctx.z_plate_mm + layer;
        let ground_tol = ctx.frame_check().tolerance_mm;

        // Ground clause: (i) touches the plate, or (ii) a TIP whose I1
        // predicate (h_delivered > 0 and meets its own requirement) is
        // established. See the module doc for what this deliberately omits
        // (the I6 seat-zone half of the full three-seat rule).
        let is_ground = |r: usize| -> bool {
            match ctx.primitives.get(r).and_then(|p| p.as_ref().ok()) {
                None => false,
                Some(p) => {
                    if p.aabb().min[2] <= z_ground + ground_tol {
                        return true;
                    }
                    if ctx.model.records[r].kind == RecordKind::ContactSphere {
                        if let Some(seat) = ctx.seat(r) {
                            if seat.indeterminate.is_none() {
                                if let (Some(hd), Some(hr)) =
                                    (seat.h_delivered_mm, seat.h_required_mm)
                                {
                                    let tol = seat.tol_geom_mm();
                                    return hd > 0.0 && hd >= hr - tol;
                                }
                            }
                        }
                    }
                    false
                }
            }
        };
        let dist = ctx.graph.distance_to_ground(&is_ground);

        let mut path_indeterminate: HashSet<usize> = HashSet::new();
        for &i in &clean_records {
            if ctx
                .graph
                .ambiguities_for(i)
                .into_iter()
                .any(|a| !matches!(a, Ambiguity::Unjoined { .. }))
            {
                path_indeterminate.insert(i);
            }
        }

        let mut tip_a_owned: HashMap<usize, f64> = HashMap::new();
        let mut tip_live: HashMap<usize, bool> = HashMap::new();
        let mut tip_indeterminate: HashSet<usize> = HashSet::new();
        for &i in &clean_records {
            if ctx.model.records[i].kind != RecordKind::ContactSphere {
                continue;
            }
            if path_indeterminate.contains(&i) {
                continue; // already indeterminate for a different reason
            }
            if ownership_budget_exceeded {
                tip_indeterminate.insert(i);
                continue;
            }
            match ctx.seat(i) {
                None => {
                    tip_indeterminate.insert(i);
                }
                Some(seat) => {
                    if seat.indeterminate.is_some() || seat.entry_component.is_none() {
                        tip_indeterminate.insert(i);
                    } else {
                        let a = ctx.a_owned_mm2(i).unwrap_or(0.0);
                        tip_a_owned.insert(i, a);
                        tip_live.insert(i, a > c::PIXEL_AREA_MM2);
                    }
                }
            }
        }

        let live_tips: Vec<usize> = tip_live
            .iter()
            .filter(|&(_, &v)| v)
            .map(|(&k, _)| k)
            .collect();

        let mut component_has_live_tip: HashSet<usize> = HashSet::new();
        for &t in &live_tips {
            component_has_live_tip.insert(ctx.graph.component_of_record[t]);
        }

        let mut dead_count = 0usize;
        let mut role_info: HashMap<usize, (bool, bool, bool, bool)> = HashMap::new();
        let mut record_status: HashMap<usize, &'static str> = HashMap::new();

        for &i in &clean_records {
            if ownership_budget_exceeded {
                outcome.indeterminate(Failure::line(
                    "GEOM_INDETERMINATE",
                    &file,
                    ctx.model_index,
                    None,
                    Some(i),
                    &[("reason", "ownership_indeterminate".to_string())],
                    {
                        let mut o = Json::obj();
                        o.set("reason", Json::str("ownership_indeterminate"));
                        o
                    },
                ));
                record_status.insert(i, "indeterminate");
                continue;
            }
            if path_indeterminate.contains(&i) {
                outcome.indeterminate(Failure::line(
                    "GEOM_INDETERMINATE",
                    &file,
                    ctx.model_index,
                    None,
                    Some(i),
                    &[("reason", "path_indeterminate".to_string())],
                    {
                        let mut o = Json::obj();
                        o.set("reason", Json::str("path_indeterminate"));
                        o
                    },
                ));
                record_status.insert(i, "indeterminate");
                continue;
            }
            if tip_indeterminate.contains(&i) {
                outcome.indeterminate(Failure::line(
                    "GEOM_INDETERMINATE",
                    &file,
                    ctx.model_index,
                    None,
                    Some(i),
                    &[("reason", "ownership_indeterminate".to_string())],
                    {
                        let mut o = Json::obj();
                        o.set("reason", Json::str("ownership_indeterminate"));
                        o
                    },
                ));
                record_status.insert(i, "indeterminate");
                continue;
            }

            let is_tip = ctx.model.records[i].kind == RecordKind::ContactSphere;
            let r1 = is_tip && tip_live.get(&i).copied().unwrap_or(false);

            let r2 = if live_tips.is_empty() {
                false
            } else {
                let reach = ctx.graph.ground_reachable_without(i, &is_ground);
                live_tips
                    .iter()
                    .any(|&t| t != i && dist[t].is_some() && !reach[t])
            };

            let r3 = ctx.graph.neighbours_of(i).iter().any(|&nb| {
                ctx.primitives.get(nb).is_some_and(Result::is_ok)
                    && !is_ground(nb)
                    && ctx.graph.neighbours_of(nb).len() == 3
            });

            let r4 = ctx.model.records[i].kind == RecordKind::RaftPadOrBridge
                && component_has_live_tip.contains(&ctx.graph.component_of_record[i]);

            role_info.insert(i, (r1, r2, r3, r4));

            if r1 || r2 || r3 || r4 {
                record_status.insert(i, "live");
                continue;
            }

            dead_count += 1;
            record_status.insert(i, "dead");
            if is_tip {
                let a_owned = tip_a_owned.get(&i).copied().unwrap_or(0.0);
                let parent = parent_record_for_tip(ctx, i);
                outcome.fail(Failure::line(
                    "GEOM_DEAD_TIP",
                    &file,
                    ctx.model_index,
                    Some(i),
                    Some(i),
                    &[
                        ("a_owned_mm2", format!("{a_owned}")),
                        ("parent_record", parent.to_string()),
                    ],
                    {
                        let mut o = Json::obj();
                        o.set("a_owned_mm2", Json::num(a_owned));
                        o.set("parent_record", Json::usize(parent));
                        o
                    },
                ));
            } else {
                let p = ctx.primitives[i]
                    .as_ref()
                    .expect("clean record has a valid primitive");
                let vol = p.volume_mm3();
                let kind_label = ctx.model.records[i].kind.label();
                outcome.fail(Failure::line(
                    "GEOM_DEAD_MEMBER",
                    &file,
                    ctx.model_index,
                    None,
                    Some(i),
                    &[
                        ("type", kind_label.to_string()),
                        ("volume_mm3", format!("{vol}")),
                        ("roles_tested", "R1,R2,R3,R4".to_string()),
                    ],
                    {
                        let mut o = Json::obj();
                        o.set("type", Json::str(kind_label));
                        o.set("volume_mm3", Json::num(vol));
                        o.set("roles_tested", Json::str("R1,R2,R3,R4"));
                        o
                    },
                ));
            }
        }

        // -------------------------------------------------------------
        // measured: every item checked, not only the failures (§5 rule 3).
        // -------------------------------------------------------------
        let mut records_json = Vec::with_capacity(n);
        for i in 0..n {
            let rec = &ctx.model.records[i];
            let mut row = Json::obj();
            row.set("record", Json::usize(i));
            row.set("type", Json::str(rec.kind.label()));
            row.set("primitive_ok", Json::Bool(ctx.primitives[i].is_ok()));
            if let Some(&agrees) = mesh_agrees.get(&i) {
                row.set("mesh_agrees", Json::Bool(agrees));
            }
            if let Some(reasons) = degenerate_reasons.get(&i) {
                row.set(
                    "degenerate_reasons",
                    Json::Arr(reasons.iter().map(|r| Json::str(*r)).collect()),
                );
            }
            if let Some(&(r1, r2, r3, r4)) = role_info.get(&i) {
                let mut roles = Json::obj();
                roles.set("r1", Json::Bool(r1));
                roles.set("r2", Json::Bool(r2));
                roles.set("r3", Json::Bool(r3));
                roles.set("r4", Json::Bool(r4));
                row.set("roles", roles);
            }
            if let Some(&a) = tip_a_owned.get(&i) {
                row.set("a_owned_mm2", Json::num(a));
            }
            let status = record_status
                .get(&i)
                .copied()
                .unwrap_or(if degenerate_reasons.contains_key(&i) || ctx.primitives[i].is_err() {
                    "degenerate"
                } else {
                    "unclassified"
                });
            row.set("status", Json::str(status));
            records_json.push(row);
        }

        let mut measured = Json::obj();
        measured.set("degenerate_count", Json::usize(degenerate_count));
        measured.set("dead_count", Json::usize(dead_count));
        measured.set("clean_records_evaluated", Json::usize(clean_records.len()));
        measured.set("live_tips", Json::usize(live_tips.len()));
        measured.set(
            "ownership_budget_exceeded",
            Json::Bool(ownership_budget_exceeded),
        );
        measured.set("records", Json::Arr(records_json));
        measured.set(
            "headline",
            Json::str(format!("{dead_count} dead, {degenerate_count} degenerate")),
        );
        outcome.measured = measured;
        outcome
    }
}

/// The `value=` field of a `GEOM_DEGENERATE` line for a field-level defect.
/// Picks the specific offending field so the number is diagnostic, not just
/// "something failed".
fn degenerate_value(e: GeomError, rec: &SupportRecord) -> f64 {
    match e {
        GeomError::ZeroLengthAxis | GeomError::ZeroLengthBridge => rec.axis_length_mm(),
        GeomError::NonPositiveRadius => {
            if rec.r_start <= 0.0 {
                rec.r_start
            } else {
                rec.r_end
            }
        }
        GeomError::NonPositiveRaftHeight => rec.r_end,
        GeomError::NonFiniteField => first_nonfinite(rec),
    }
}

/// The first non-finite component among a record's own fields, in
/// `start, end, r_start, r_end` order. `NaN`/`inf` have no JSON spelling, so
/// this is reported as a string rather than laundered into a number.
fn first_nonfinite(rec: &SupportRecord) -> f64 {
    for v in rec.start_mm.into_iter().chain(rec.end_mm) {
        if !v.is_finite() {
            return v;
        }
    }
    if !rec.r_start.is_finite() {
        return rec.r_start;
    }
    if !rec.r_end.is_finite() {
        return rec.r_end;
    }
    f64::NAN
}

/// A JSON number when finite, else a string -- `Json::num` panics on
/// non-finite values by design (`json.rs` module doc), and a non-finite
/// degenerate value is exactly the case this check exists to surface, not to
/// crash on.
fn value_json(v: f64) -> Json {
    if v.is_finite() {
        Json::num(v)
    } else {
        Json::str(format!("{v}"))
    }
}

/// The structural parent of a dead TIP for `GEOM_DEAD_TIP`'s `parent_record`:
/// the taper's own further neighbour that is a Member or RootFlare (the arm
/// actually holding it), falling back to the taper itself. Only called once
/// the tip's seat has already been established as non-indeterminate, which
/// guarantees a unique adjacent taper (`seat.rs`: a missing or ambiguous
/// taper makes the seat indeterminate before this point is ever reached).
fn parent_record_for_tip(ctx: &ModelContext, tip: usize) -> usize {
    if let Some(seat) = ctx.seat(tip) {
        if let Link::Unique(taper) = seat.taper {
            for &nb in ctx.graph.neighbours_of(taper) {
                if nb == tip {
                    continue;
                }
                if matches!(
                    ctx.model.records[nb].kind,
                    RecordKind::Member | RecordKind::RootFlare
                ) {
                    return nb;
                }
            }
            return taper;
        }
    }
    tip
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{AuditContext, AuditOptions};
    use crate::reader::ChituboxFile;
    use crate::synth::{box_soup, frustum_soup, SynthFile, SynthModel, SynthRecord};
    use std::path::Path;

    /// A closed UV sphere with a single-triangle fan at each pole, unlike
    /// `crate::synth::sphere_soup` (shared, not owned by this file), which
    /// collapses every longitude to one bit-identical point at `la = 0`
    /// (`sin(0.0)` is exact) and therefore ships `lon` EXACTLY zero-area
    /// triangles at its north pole -- a real defect in the shared test
    /// harness that this file may not fix. Building the tip's own soup here
    /// keeps the "fully clean" fixture actually clean.
    fn tip_sphere_soup(centre: [f64; 3], r: f64, lat: usize, lon: usize) -> Vec<[[f32; 3]; 3]> {
        let pt = |la: usize, lo: usize| -> [f32; 3] {
            let th = std::f64::consts::PI * la as f64 / lat as f64;
            let ph = 2.0 * std::f64::consts::PI * (lo % lon) as f64 / lon as f64;
            [
                (centre[0] + r * th.sin() * ph.cos()) as f32,
                (centre[1] + r * th.sin() * ph.sin()) as f32,
                (centre[2] + r * th.cos()) as f32,
            ]
        };
        let north = pt(0, 0);
        let south = pt(lat, 0);
        let mut tris = Vec::new();
        for lo in 0..lon {
            tris.push([north, pt(1, lo), pt(1, lo + 1)]);
        }
        for la in 1..lat - 1 {
            for lo in 0..lon {
                let (a, b, cc, d) = (pt(la, lo), pt(la + 1, lo), pt(la + 1, lo + 1), pt(la, lo + 1));
                tris.push([a, b, cc]);
                tris.push([a, cc, d]);
            }
        }
        for lo in 0..lon {
            tris.push([south, pt(lat - 1, lo + 1), pt(lat - 1, lo)]);
        }
        tris
    }

    /// A minimal, fully clean chain: tip -> taper -> member -> pad, every
    /// record's soup matching its analytic form, the tip embedded, the whole
    /// chain grounded through the pad. This is the boundary the predicate
    /// must sit exactly on: nothing here is degenerate and nothing is dead.
    fn clean_chain_file() -> SynthFile {
        let pad_base = [0.0, 0.0, -7.0];
        let pad_r = 1.1; // r_start (2.2, a DIAMETER) / 2
        let pad_h = 0.2; // r_end
        SynthFile::new().with_model(SynthModel {
            name: "clean".into(),
            translate_mm: [0.0, 0.0, 5.0],
            mesh: box_soup([-5.0, -5.0, -2.0], [5.0, 5.0, 2.0]),
            records: vec![
                SynthRecord::new(
                    RecordKind::ContactSphere,
                    [0.0, 0.0, -1.9],
                    [0.0, 0.0, -2.8],
                    0.45,
                    0.45,
                )
                .with_soup(tip_sphere_soup([0.0, 0.0, -2.35], 0.45, 8, 16)),
                SynthRecord::new(
                    RecordKind::ContactTaper,
                    [0.0, 0.0, -2.35],
                    [0.0, 0.0, -4.0],
                    0.3,
                    0.35,
                )
                .with_declared_depth(0.1)
                .with_matching_soup(),
                SynthRecord::new(
                    RecordKind::Member,
                    [0.0, 0.0, -4.0],
                    pad_base,
                    0.35,
                    0.35,
                )
                .with_matching_soup(),
                SynthRecord::new(
                    RecordKind::RaftPadOrBridge,
                    pad_base,
                    pad_base,
                    2.2,
                    0.2,
                )
                .with_soup(frustum_soup(
                    pad_base,
                    pad_r,
                    [pad_base[0], pad_base[1], pad_base[2] + pad_h],
                    pad_r,
                    16,
                ))
                .as_pad(),
            ],
        })
    }

    fn run_check(bytes: &[u8]) -> (CheckOutcome, crate::json::Json) {
        let file = ChituboxFile::parse(Path::new("t.chitubox"), bytes).expect("parse");
        let options = AuditOptions::default();
        let audit = AuditContext {
            file: &file,
            options: &options,
            sidecar: None,
            sidecar_absence: Some("none".into()),
        };
        let ctx = ModelContext::build(&audit, 0);
        let outcome = DeadGeometry.run(&ctx);
        let json = outcome.to_json();
        (outcome, json)
    }

    #[test]
    fn a_fully_clean_grounded_chain_has_no_degenerate_and_no_dead_geometry() {
        let (outcome, _) = run_check(&clean_chain_file().to_bytes());
        assert_eq!(
            outcome
                .measured
                .get("degenerate_count")
                .and_then(crate::json::Json::as_f64),
            Some(0.0),
            "{:?}",
            outcome.failures
        );
        assert_eq!(
            outcome
                .measured
                .get("dead_count")
                .and_then(crate::json::Json::as_f64),
            Some(0.0),
            "{:?}",
            outcome.failures
        );
        assert_eq!(outcome.status().as_str(), "pass", "{:?}", outcome.failures);
    }

    #[test]
    fn a_zero_length_member_is_reported_degenerate_with_no_tolerance() {
        let mut f = clean_chain_file();
        f.models[0].records[2].end_mm = f.models[0].records[2].start_mm; // zero-length member
        let (outcome, _) = run_check(&f.to_bytes());
        assert!(outcome
            .failures
            .iter()
            .any(|fl| fl.code == "GEOM_DEGENERATE" && fl.message.contains("reason=zero_length")));
        assert_eq!(outcome.status().as_str(), "fail");
    }

    #[test]
    fn a_nonpositive_radius_is_degenerate_at_the_exact_boundary() {
        let mut f = clean_chain_file();
        // Exactly zero, not "close to zero": the predicate is `<= 0`, no slack.
        f.models[0].records[2].r_start = 0.0;
        let (outcome, _) = run_check(&f.to_bytes());
        assert!(outcome.failures.iter().any(|fl| fl.code == "GEOM_DEGENERATE"
            && fl.message.contains("reason=nonpositive_radius")));
    }

    #[test]
    fn an_empty_soup_is_degenerate_and_excluded_from_liveness() {
        let mut f = clean_chain_file();
        f.models[0].records[2].soup.clear();
        let (outcome, _) = run_check(&f.to_bytes());
        assert!(outcome
            .failures
            .iter()
            .any(|fl| fl.code == "GEOM_DEGENERATE" && fl.message.contains("reason=empty_soup")));
        // An empty-soup record must not ALSO be graded dead: it already
        // failed for a different, more specific reason.
        assert!(!outcome
            .failures
            .iter()
            .any(|fl| fl.code == "GEOM_DEAD_MEMBER" && fl.record == Some(2)));
    }

    #[test]
    fn a_mesh_that_disagrees_with_its_record_is_a_named_finding() {
        let mut f = clean_chain_file();
        // Scale the member's soup vertices without changing its record: the
        // shipped triangles no longer describe the solid the fields claim.
        for tri in &mut f.models[0].records[2].soup {
            for v in tri {
                v[0] *= 3.0;
            }
        }
        let (outcome, _) = run_check(&f.to_bytes());
        assert!(outcome
            .failures
            .iter()
            .any(|fl| fl.code == "GEOM_MESH_DISAGREES_WITH_RECORD" && fl.record == Some(2)));
    }

    #[test]
    fn a_dangling_member_with_no_tip_and_no_bracing_role_is_dead() {
        // A fifth record: a spur off the raft pad's own connective point,
        // reaching sideways into free space. It carries no tip (not R1), its
        // removal disconnects nothing (the live tip's path never touches it,
        // not R2), it does not turn a non-grounded 3-way junction into a
        // pass-through (its only neighbour, the pad, is itself grounded and
        // therefore excluded from R3 credit by construction), and it is not
        // a raft primitive (not R4). None of R1-R4 apply, so it must be
        // reported dead -- the direct, load-bearing-in-name-only case R1-R4
        // exist to catch.
        let mut f = clean_chain_file();
        let pad_point = [0.0, 0.0, -7.0];
        let spur_end = [3.0, 0.0, -7.0];
        f.models[0].records.push(
            SynthRecord::new(RecordKind::Member, pad_point, spur_end, 0.2, 0.2)
                .with_matching_soup(),
        );
        let (outcome, _) = run_check(&f.to_bytes());
        assert_eq!(
            outcome.measured.get("degenerate_count").and_then(Json::as_f64),
            Some(0.0),
            "the spur itself is well-formed geometry, not degenerate: {:?}",
            outcome.failures
        );
        assert!(
            outcome
                .failures
                .iter()
                .any(|fl| fl.code == "GEOM_DEAD_MEMBER" && fl.record == Some(4)),
            "the spur must be reported dead: {:?}",
            outcome.failures
        );
        // And the live chain must NOT be caught in the same net: the tip,
        // taper, member and pad it depends on must all still read live.
        for r in [0usize, 1, 2, 3] {
            assert!(
                !outcome
                    .failures
                    .iter()
                    .any(|fl| (fl.code == "GEOM_DEAD_MEMBER" || fl.code == "GEOM_DEAD_TIP")
                        && fl.record == Some(r)),
                "record {r} of the live chain must not be caught dead by the spur's presence: {:?}",
                outcome.failures
            );
        }
    }

    #[test]
    fn the_boundary_of_pixel_area_censor_matches_the_constant() {
        // The predicate is `A_owned(c) > pixel_area`, strict. Confirm the
        // constant this check reads is exactly the one the specification
        // derives, so a future edit to consts.rs cannot silently loosen R1.
        assert!((c::PIXEL_AREA_MM2 - 2.66e-4).abs() < 1e-9);
    }

    #[test]
    fn an_unmeasurable_case_is_indeterminate_never_a_silent_pass() {
        // Force the ownership partition into its own budget-exceeded state
        // and confirm every clean record in the model is reported
        // indeterminate rather than being graded on a guess.
        let options = AuditOptions {
            sample_budget: crate::ownership::SampleBudget {
                max_samples: 1,
                ..crate::ownership::SampleBudget::default()
            },
            ..AuditOptions::default()
        };
        let bytes = clean_chain_file().to_bytes();
        let file = ChituboxFile::parse(Path::new("t.chitubox"), &bytes).expect("parse");
        let audit = AuditContext {
            file: &file,
            options: &options,
            sidecar: None,
            sidecar_absence: Some("none".into()),
        };
        let ctx = ModelContext::build(&audit, 0);
        assert!(
            ctx.ownership().budget_exceeded.is_some(),
            "the test setup must actually exhaust the budget"
        );
        let outcome = DeadGeometry.run(&ctx);
        assert_eq!(outcome.status().as_str(), "fail");
        assert!(
            outcome.indeterminate > 0,
            "an unmeasurable ownership partition must be reported indeterminate, \
             never silently graded"
        );
        assert!(outcome
            .failures
            .iter()
            .any(|fl| fl.code == "GEOM_INDETERMINATE"
                && fl.message.contains("reason=ownership_indeterminate")));
    }

    #[test]
    fn value_json_never_hands_a_nonfinite_number_to_the_json_writer() {
        let j = value_json(f64::NAN);
        assert_eq!(j.as_str(), Some("NaN"));
        let j = value_json(0.0);
        assert_eq!(j.as_f64(), Some(0.0));
    }
}
