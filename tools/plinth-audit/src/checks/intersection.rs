//! I6 no intersection -- `docs/specs/audit-invariants.md` §3.6.
//!
//! Two claims, both graded on the **cured raster** so the volumes are exact
//! voxel sums and the censor is a count of one.
//!
//! * **Clause (a)** -- `volume((S ∩ M) minus ∪ Z(c)) <= voxel_volume`. No
//!   support material inside the model outside its own SEAT ZONE. The seat zone
//!   is a bounded volume around the contact's own axis, deliberately NOT the
//!   engine's connected-surface-component exemption of `docs/physics.md` §6: a
//!   volume bound cannot be widened by a fat arm and cannot swallow a far wall.
//! * **Clause (b)** -- for every contact, the components of `S(c) minus M`
//!   number exactly one, where `S(c)` is the tip group's solid. A tip that
//!   punches through a thin wall leaves a SECOND blob of support material
//!   outside the model on the far side, so counting exterior components is
//!   exactly the punch-through question and it needs no wall-thickness
//!   measurement.
//!
//! Nothing here refuses the accepted crevice-contact ruling of 2026-08-25: an
//! arm touching the opposite wall of a narrow crevice *touches*, and touching
//! contributes no interior volume. Only material that is genuinely **inside**
//! the model counts, which is the ruling's own line.
//!
//! ## Deviations from the specification, stated rather than hidden
//!
//! 1. **Two extra `reason=` tokens.** §3.6 pins
//!    `ray_parity_exhausted|seat_indeterminate|raster_budget_exceeded`. Two
//!    conditions fit none of the three and both are genuinely unestablished, so
//!    they get their own tokens rather than a wrong one or a silent pass:
//!    `tip_group_unrendered` (the tip group cures no voxel at all, so clause
//!    (b) has nothing to count) and `unattributable_voxel` (an illegal voxel no
//!    primitive claims, so the failure line's `record=` cannot be filled).
//! 2. **Clause (b) censors exterior components at one voxel**, the same censor
//!    §3.6 writes into clause (a). A blob below one voxel is not rendered, and
//!    without the censor a single staircase voxel would invent a breach.
//! 3. **Connectivity for clause (b) is 26-connected**, not 6-connected. Two
//!    voxels meeting along an edge are cured by overlapping exposures and are
//!    one piece; splitting them would manufacture breaches out of raster
//!    staircase artefacts. The error direction is toward *fewer* reported
//!    breaches, which is disclosed in `measured.connectivity`.
//! 4. **The zone membership test samples the voxel CENTRE** and dilates the
//!    zone by the voxel circumradius `d3/2 = 2.7645e-2 mm`, which is the
//!    sampling resolution itself. §3.6's radial `+ d2` is subsumed by it
//!    (`d3/2 > d2`). Without the dilation every seat would shed a one-voxel rim
//!    of false violations.

use crate::check::{Check, CheckOutcome, Failure};
use crate::consts as c;
use crate::context::ModelContext;
use crate::graph::Link;
use crate::json::Json;
use crate::primitives::Primitive;
use crate::raster::{RasterError, RasterGrid, Run, RunRaster};
use crate::reader::RecordKind;
use crate::vec3::{self as v, Aabb, V3};
use std::collections::BTreeMap;

/// Enumerating `S ∩ M` voxel by voxel is linear in the intersection, not in
/// the model, so the cap is generous. Exceeding it is
/// `raster_budget_exceeded`, never a truncated answer.
const MAX_INTERSECTION_VOXELS: u64 = 12_000_000;

/// Cell edge of the spatial hash over seat zones and over primitives, mm. It
/// only has to exceed the largest zone/primitive bbox that must be found from
/// a point inside it; a coarser cell costs candidates, never correctness.
const HASH_CELL_MM: f64 = 2.0;

/// Centreline sampling step for the buried-run diagnostic, mm. Half a layer,
/// so a buried run one layer long cannot be stepped over.
const CENTRELINE_STEP_MM: f64 = c::LAYER_HEIGHT_DEFAULT_MM * 0.5;

pub struct Intersection;

impl Check for Intersection {
    fn id(&self) -> &'static str {
        "i6_no_intersection"
    }

    fn title(&self) -> &'static str {
        "I6 no intersection"
    }

    fn scope(&self, ctx: &ModelContext) -> usize {
        ctx.model.records.len()
    }

    fn run(&self, ctx: &ModelContext) -> CheckOutcome {
        let file = ctx.file();
        let model = ctx.model_index;
        let n_records = ctx.model.records.len();
        let mut out = CheckOutcome::new(n_records);
        let grid = ctx.grid;
        let voxel_mm3 = grid.voxel_volume_mm3();
        let layer_h = ctx.inputs().layer_height_mm;
        let margin = c::D3_MM * 0.5;

        if n_records == 0 {
            let mut me = Json::obj();
            me.set("implemented", Json::Bool(true));
            me.set("support_records", Json::usize(0));
            me.set(
                "headline",
                Json::str("no support records: no support solid can be inside the model"),
            );
            out.measured = me;
            return out;
        }

        // A model that shipped no triangles has no interior, so "is the
        // support inside it" has no answer. `RunRaster::from_mesh` returns an
        // EMPTY raster for an empty mesh rather than an error, and an empty
        // model would make both clauses pass by vacuity -- exactly the shape
        // of the defect this tool exists to refuse. Refuse it here.
        if ctx.model_mesh.is_empty() {
            out.indeterminate(indeterminate_line(
                &file,
                model,
                0,
                "ray_parity_exhausted",
                Some("the model shipped no triangles: it has no interior to be inside of"),
            ));
            out.indeterminate = n_records;
            let mut me = Json::obj();
            me.set("implemented", Json::Bool(true));
            me.set("model_triangles", Json::usize(0));
            me.set("support_records", Json::usize(n_records));
            me.set(
                "headline",
                Json::str(
                    "the model shipped no triangles; inside/outside is undefined and \
                     nothing was established",
                ),
            );
            out.measured = me;
            return out;
        }

        // -------------------------------------------------------------------
        // The two cured solids. An error is never absorbed into a pass.
        // -------------------------------------------------------------------
        let support = match ctx.support_raster() {
            Ok(r) => r,
            Err(e) => return all_indeterminate(out, &file, model, n_records, "support", e),
        };
        let model_r = match ctx.model_raster() {
            Ok(r) => r,
            Err(e) => return all_indeterminate(out, &file, model, n_records, "model", e),
        };

        // -------------------------------------------------------------------
        // Seat zones, one per contact, and the records they exempt.
        // -------------------------------------------------------------------
        let mut zones: Vec<Zone> = Vec::new();
        let mut exempt_records: Vec<bool> = vec![false; n_records];
        let mut unestablished_contacts = 0usize;

        for seat in &ctx.seats {
            let tip_record = seat.contact_id;
            let tip_prim = ctx.primitive(tip_record).ok().copied();
            let taper_link = ctx.graph.taper_for_tip(ctx.model, tip_record);
            let taper_record = taper_link.unique();
            let taper_prim = taper_record.and_then(|t| ctx.primitive(t).ok().copied());

            let zone_ok = seat.u.is_some() && seat.h_delivered_mm.is_some() && tip_prim.is_some();
            if !zone_ok {
                // The zone is not established, so nothing near this contact can
                // be graded against it. Report it, and take the contact's own
                // solids out of clause (a) rather than convict them on a zone
                // the audit could not build.
                unestablished_contacts += 1;
                let reason = match seat.indeterminate {
                    Some("ray_parity_exhausted") => "ray_parity_exhausted",
                    _ => "seat_indeterminate",
                };
                out.indeterminate(indeterminate_line(
                    &file,
                    model,
                    tip_record,
                    reason,
                    Some(seat.indeterminate.unwrap_or("zone_unbuildable")),
                ));
                exempt_records[tip_record] = true;
                if let Some(t) = taper_record {
                    exempt_records[t] = true;
                }
                continue;
            }
            if !matches!(taper_link, Link::Unique(_)) {
                // The tip GROUP is not established: §3.6 defines it as the TIP
                // plus the adjacent TAPER, and there is no unique adjacent
                // taper. The ball's own zone is still built (which is the
                // stricter reading), and every taper the graph joins to this
                // tip is taken out of clause (a) so the missing link does not
                // masquerade as buried geometry.
                unestablished_contacts += 1;
                out.indeterminate(indeterminate_line(
                    &file,
                    model,
                    tip_record,
                    "seat_indeterminate",
                    Some(taper_link.reason()),
                ));
                for &nb in ctx.graph.neighbours_of(tip_record) {
                    if ctx.model.records[nb].kind == RecordKind::ContactTaper {
                        exempt_records[nb] = true;
                    }
                }
            }

            let u = seat.u.expect("zone_ok checked u");
            let h = seat.h_delivered_mm.expect("zone_ok checked h");
            let tip = tip_prim.expect("zone_ok checked the tip solid");
            let mut bbox = tip.aabb();
            if let Some(tp) = taper_prim {
                bbox.union(&tp.aabb());
            }
            zones.push(Zone {
                contact: tip_record,
                p0: seat.p0,
                u,
                t_max: h + layer_h,
                tip,
                taper: taper_prim,
                bbox: bbox.expanded(margin),
            });
        }

        let zone_hash = SpatialHash::build(zones.iter().map(|z| z.bbox));
        let contact_points: Vec<(usize, V3)> =
            ctx.seats.iter().map(|s| (s.contact_id, s.p0)).collect();

        // -------------------------------------------------------------------
        // Clause (a): the intersection, minus every seat zone.
        // -------------------------------------------------------------------
        let live: Vec<(usize, Primitive)> = ctx.live_primitives();
        let prim_hash = SpatialHash::build(live.iter().map(|(_, p)| p.aabb().expanded(margin)));
        // Each record's own centreline frame, so an illegal voxel can be given
        // a station along the solid it belongs to.
        let mut axis_of: Vec<Option<(V3, V3)>> = vec![None; n_records];
        for &(ri, p) in &live {
            let (a, b) = p.centreline();
            if let Some(u) = v::normalize(v::sub(b, a)) {
                axis_of[ri] = Some((a, u));
            }
        }
        // A ball has no centreline of its own. A TIP ball does have an axis --
        // the seat axis -- and it is the one a burial depth means anything
        // along, so use it rather than leaving the station undefined.
        for seat in &ctx.seats {
            if let Some(u) = seat.u {
                axis_of[seat.contact_id] = Some((seat.p0, u));
            }
        }

        let inter = support.intersect(model_r);
        if inter.pixel_count > MAX_INTERSECTION_VOXELS {
            out.indeterminate(indeterminate_line(
                &file,
                model,
                0,
                "raster_budget_exceeded",
                Some("intersection_voxels"),
            ));
            out.indeterminate = n_records;
            let mut me = Json::obj();
            me.set("implemented", Json::Bool(true));
            me.set("intersection_voxels", Json::num(inter.pixel_count as f64));
            me.set("voxel_budget", Json::num(MAX_INTERSECTION_VOXELS as f64));
            me.set(
                "headline",
                Json::str("intersection exceeds the voxel budget; nothing established"),
            );
            out.measured = me;
            return out;
        }

        let mut inside_voxels: u64 = 0;
        let mut exempt_voxels: u64 = 0;
        let mut illegal_per_record: BTreeMap<usize, VoxAcc> = BTreeMap::new();
        let mut seat_zone_voxels: BTreeMap<usize, u64> = BTreeMap::new();
        let mut unattributed = VoxAcc::default();

        for layer in &inter.layers {
            for (&row, runs) in &layer.rows {
                for r in runs {
                    for col in r.c0..r.c1 {
                        inside_voxels += 1;
                        let p = voxel_centre(&grid, layer.k, row, col);
                        let mut exempt: Option<usize> = None;
                        for zi in zone_hash.candidates(p) {
                            if zones[zi].contains(p, margin) {
                                exempt = Some(zones[zi].contact);
                                break;
                            }
                        }
                        if let Some(cid) = exempt {
                            exempt_voxels += 1;
                            *seat_zone_voxels.entry(cid).or_default() += 1;
                            continue;
                        }
                        // Attribute the illegal voxel to the record whose own
                        // solid claims it most strongly.
                        let mut best: Option<(usize, f64)> = None;
                        for pi in prim_hash.candidates(p) {
                            let (ri, prim) = live[pi];
                            let sd = prim.signed_distance(p);
                            if best.is_none_or(|(_, bd)| sd < bd) {
                                best = Some((ri, sd));
                            }
                        }
                        match best {
                            Some((ri, _)) if !exempt_records[ri] => {
                                let t = axis_of[ri]
                                    .map_or(0.0, |(a, u)| v::dot(v::sub(p, a), u));
                                illegal_per_record.entry(ri).or_default().add(p, t);
                            }
                            Some(_) => {}
                            None => unattributed.add(p, 0.0),
                        }
                    }
                }
            }
        }

        let illegal_voxels: u64 = illegal_per_record.values().map(|a| a.count).sum::<u64>()
            + unattributed.count;
        let residual_mm3 = illegal_voxels as f64 * voxel_mm3;

        if unattributed.count > 0 {
            out.indeterminate(indeterminate_line(
                &file,
                model,
                0,
                "unattributable_voxel",
                Some("no primitive claims this cured voxel"),
            ));
        }

        // The predicate is on the TOTAL, censored at one voxel.
        //
        // EXEMPT from the interval policy (`audit-interval-policy.md` §1.6,
        // §3.4): `residual_mm3` is an EXACT voxel count times the voxel
        // volume, and the censor is a count of one. I6 is graded on the cured
        // raster precisely so its quantities are exact integer sums; there is
        // no continuous uncertainty and therefore no interval to widen.
        if residual_mm3 > voxel_mm3 {
            for (&ri, acc) in &illegal_per_record {
                let vol = acc.count as f64 * voxel_mm3;
                let ctr = acc.centroid();
                let (near_c, near_d) = nearest_contact(&contact_points, ctr);
                out.fail(Failure::line(
                    "INTERSECT_OUTSIDE_SEAT",
                    &file,
                    model,
                    None,
                    Some(ri),
                    &[
                        ("type", ctx.model.records[ri].raw_type.to_string()),
                        ("volume_mm3", num(vol)),
                        ("censor_mm3", format!("{voxel_mm3:.2e}")),
                        ("centroid", v3(ctr)),
                        (
                            "nearest_contact",
                            near_c.map_or_else(|| "none".to_string(), |x| x.to_string()),
                        ),
                        // A model can carry support records and no TIP at all;
                        // there is then no nearest contact and no distance, and
                        // the line says so rather than printing an infinity.
                        (
                            "distance_mm",
                            if near_c.is_some() {
                                num(near_d)
                            } else {
                                "none".to_string()
                            },
                        ),
                    ],
                    {
                        let mut o = Json::obj();
                        o.set("wire_type", Json::usize(ctx.model.records[ri].raw_type as usize));
                        o.set("kind", Json::str(ctx.model.records[ri].kind.label()));
                        o.set("volume_mm3", Json::num(vol));
                        o.set("voxels", Json::num(acc.count as f64));
                        o.set("censor_mm3", Json::num(voxel_mm3));
                        o.set("centroid_mm", Json::v3(ctr));
                        o.set(
                            "nearest_contact",
                            near_c.map_or(Json::Null, Json::usize),
                        );
                        o.set(
                            "distance_mm",
                            Json::num_or_null(near_c.map(|_| near_d)),
                        );
                        o.set("buried_axial_span_mm", Json::num(acc.axial_span_mm()));
                        o.set("buried_extent_mm", Json::num(acc.extent_mm()));
                        o
                    },
                ));
            }
        }

        // -------------------------------------------------------------------
        // Clause (b): the far-side test, per contact.
        // -------------------------------------------------------------------
        let mut per_contact: Vec<Json> = Vec::with_capacity(ctx.seats.len());
        let mut worst_components = 0usize;
        let mut worst_breach_mm3 = 0.0f64;
        let mut breaches = 0usize;

        for seat in &ctx.seats {
            let tip_record = seat.contact_id;
            let mut prims: Vec<Primitive> = Vec::with_capacity(2);
            if let Ok(p) = ctx.primitive(tip_record) {
                prims.push(*p);
            }
            let taper_record = ctx.graph.taper_for_tip(ctx.model, tip_record).unique();
            if let Some(t) = taper_record {
                if let Ok(p) = ctx.primitive(t) {
                    prims.push(*p);
                }
            }
            if prims.is_empty() {
                out.indeterminate(indeterminate_line(
                    &file,
                    model,
                    tip_record,
                    "tip_group_unrendered",
                    Some("no analytic solid for the tip group"),
                ));
                continue;
            }
            let group = match RunRaster::from_primitives(
                &prims,
                grid,
                &ctx.audit.options.raster_budget,
            ) {
                Ok(r) => r,
                Err(_) => {
                    out.indeterminate(indeterminate_line(
                        &file,
                        model,
                        tip_record,
                        "raster_budget_exceeded",
                        Some("tip group"),
                    ));
                    continue;
                }
            };
            if group.pixel_count == 0 {
                out.indeterminate(indeterminate_line(
                    &file,
                    model,
                    tip_record,
                    "tip_group_unrendered",
                    Some("the tip group cures no voxel under full containment"),
                ));
                continue;
            }

            let exterior = subtract_raster(&group, model_r);
            let comps = components_26(&exterior, &grid);
            let kept: Vec<&Component> = comps.iter().filter(|k| k.count as f64 * voxel_mm3 > voxel_mm3).collect();
            let n_kept = kept.len();
            worst_components = worst_components.max(n_kept);

            // The primary component is the one the arm leaves by: the one
            // nearest the taper's far end, or failing that the largest.
            let anchor = taper_record.map(|t| ctx.model.records[t].end_mm);
            let primary = pick_primary(&kept, anchor);
            let total_ext = exterior.pixel_count as f64 * voxel_mm3;
            let primary_vol = primary.map_or(0.0, |p| kept[p].count as f64 * voxel_mm3);
            let breach_vol = kept
                .iter()
                .enumerate()
                .filter(|(i, _)| Some(*i) != primary)
                .map(|(_, k)| k.count as f64 * voxel_mm3)
                .sum::<f64>();
            let breach_vol = nz(breach_vol);
            let breach_centroid = kept
                .iter()
                .enumerate()
                .filter(|(i, _)| Some(*i) != primary)
                .max_by(|a, b| a.1.count.cmp(&b.1.count))
                .map(|(_, k)| k.centroid());

            let mut row = Json::obj();
            row.set("contact", Json::usize(tip_record));
            row.set("tip_group_records", Json::usize(prims.len()));
            row.set("tip_group_volume_mm3", Json::num(group.pixel_count as f64 * voxel_mm3));
            row.set("exterior_volume_mm3", Json::num(total_ext));
            row.set(
                "seat_zone_volume_mm3",
                Json::num(
                    seat_zone_voxels.get(&tip_record).copied().unwrap_or(0) as f64 * voxel_mm3,
                ),
            );
            row.set("exterior_components", Json::usize(n_kept));
            row.set("exterior_components_uncensored", Json::usize(comps.len()));
            row.set("primary_volume_mm3", Json::num(primary_vol));
            row.set("breach_volume_mm3", Json::num(breach_vol));
            per_contact.push(row);

            if n_kept != 1 {
                breaches += 1;
                worst_breach_mm3 = worst_breach_mm3.max(breach_vol);
                let ctr = breach_centroid.unwrap_or(seat.p0);
                out.fail(Failure::line(
                    "INTERSECT_FAR_SIDE_BREACH",
                    &file,
                    model,
                    Some(tip_record),
                    Some(tip_record),
                    &[
                        ("exterior_components", n_kept.to_string()),
                        ("breach_volume_mm3", num(breach_vol)),
                        ("breach_centroid", v3(ctr)),
                    ],
                    {
                        let mut o = Json::obj();
                        o.set("exterior_components", Json::usize(n_kept));
                        o.set("breach_volume_mm3", Json::num(breach_vol));
                        o.set("breach_centroid_mm", Json::v3(ctr));
                        o.set("exterior_volume_mm3", Json::num(total_ext));
                        o.set("primary_volume_mm3", Json::num(primary_vol));
                        o.set("censor_mm3", Json::num(voxel_mm3));
                        o.set(
                            "note",
                            Json::str(if n_kept == 0 {
                                "the tip group cures nothing outside the model: it is wholly enclosed"
                            } else {
                                "the tip group cures a second blob outside the model"
                            }),
                        );
                        o
                    },
                ));
            }
        }

        // -------------------------------------------------------------------
        // The buried-centreline diagnostic: reproduce or refute the prior probe
        // (0.45-5.34 % of records, worst buried run 0.586 mm).
        // -------------------------------------------------------------------
        let buried = buried_runs(ctx, model_r);

        // -------------------------------------------------------------------
        // measured -- every quantity the failure lines name, for every item.
        // -------------------------------------------------------------------
        let mut per_record: Vec<Json> = Vec::with_capacity(n_records);
        for ri in 0..n_records {
            let acc = illegal_per_record.get(&ri);
            let mut o = Json::obj();
            o.set("record", Json::usize(ri));
            o.set("wire_type", Json::usize(ctx.model.records[ri].raw_type as usize));
            o.set("kind", Json::str(ctx.model.records[ri].kind.label()));
            o.set(
                "outside_seat_volume_mm3",
                Json::num(acc.map_or(0.0, |a| a.count as f64 * voxel_mm3)),
            );
            o.set(
                "outside_seat_voxels",
                Json::num(acc.map_or(0.0, |a| a.count as f64)),
            );
            o.set(
                "buried_axial_span_mm",
                Json::num(acc.map_or(0.0, VoxAcc::axial_span_mm)),
            );
            o.set(
                "buried_extent_mm",
                Json::num(acc.map_or(0.0, VoxAcc::extent_mm)),
            );
            o.set("buried_run_raster_mm", Json::num(buried.per_record[ri]));
            o.set("buried_run_mesh_mm", Json::num(buried.per_record_mesh[ri]));
            o.set("seat_zone_exempt", Json::Bool(exempt_records[ri]));
            per_record.push(o);
        }

        let mut me = Json::obj();
        me.set("implemented", Json::Bool(true));
        me.set("support_records", Json::usize(n_records));
        me.set("contacts", Json::usize(ctx.seats.len()));
        me.set("seat_zones_built", Json::usize(zones.len()));
        me.set("contacts_unestablished", Json::usize(unestablished_contacts));
        me.set("censor_mm3", Json::num(voxel_mm3));
        me.set("zone_dilation_mm", Json::num(margin));
        me.set("connectivity", Json::str("26-connected, censored at one voxel"));

        let mut a = Json::obj();
        a.set("intersection_volume_mm3", Json::num(inside_voxels as f64 * voxel_mm3));
        a.set("intersection_voxels", Json::num(inside_voxels as f64));
        a.set("in_seat_zone_volume_mm3", Json::num(exempt_voxels as f64 * voxel_mm3));
        a.set("outside_seat_volume_mm3", Json::num(residual_mm3));
        a.set("outside_seat_voxels", Json::num(illegal_voxels as f64));
        a.set("unattributed_voxels", Json::num(unattributed.count as f64));
        a.set("records_with_illegal_volume", Json::usize(illegal_per_record.len()));
        a.set(
            "fraction_of_records_with_illegal_volume",
            Json::num(illegal_per_record.len() as f64 / n_records as f64),
        );
        a.set(
            "max_buried_axial_span_mm",
            Json::num(
                illegal_per_record
                    .values()
                    .map(VoxAcc::axial_span_mm)
                    .fold(0.0f64, f64::max),
            ),
        );
        a.set(
            "max_buried_extent_mm",
            Json::num(
                illegal_per_record
                    .values()
                    .map(VoxAcc::extent_mm)
                    .fold(0.0f64, f64::max),
            ),
        );
        a.set("passes", Json::Bool(residual_mm3 <= voxel_mm3));
        me.set("clause_a", a);

        let mut b = Json::obj();
        b.set("contacts_checked", Json::usize(per_contact.len()));
        b.set("breaches", Json::usize(breaches));
        b.set("max_exterior_components", Json::usize(worst_components));
        b.set("worst_breach_volume_mm3", Json::num(worst_breach_mm3));
        b.set("per_contact", Json::Arr(per_contact));
        me.set("clause_b", b);

        me.set("buried_centreline", buried.to_json(n_records));
        me.set("per_record", Json::Arr(per_record));
        me.set(
            "headline",
            Json::str(format!(
                "outside-seat {} mm3 over {:.2e} censor in {} of {} records ({:.2} %), worst buried span {} mm axial / {} mm extent | worst {} exterior components, breach {} mm3 | centreline burial {:.2} % of records, worst run {} mm raster / {} mm mesh",
                num(residual_mm3),
                voxel_mm3,
                illegal_per_record.len(),
                n_records,
                100.0 * illegal_per_record.len() as f64 / n_records as f64,
                num(
                    illegal_per_record
                        .values()
                        .map(VoxAcc::axial_span_mm)
                        .fold(0.0f64, f64::max)
                ),
                num(
                    illegal_per_record
                        .values()
                        .map(VoxAcc::extent_mm)
                        .fold(0.0f64, f64::max)
                ),
                worst_components,
                num(worst_breach_mm3),
                buried.fraction_of_records() * 100.0,
                num(buried.max_run_mm),
                num(buried.mesh_max_run_mm),
            )),
        );
        out.measured = me;
        out
    }
}

// ---------------------------------------------------------------------------
// The seat zone
// ---------------------------------------------------------------------------

/// `Z(c)` of `audit-invariants.md` §3.6: the part of the intersection inside
/// the contact's own footprint, between the ball's deepest point and one layer
/// past the delivered entry.
struct Zone {
    contact: usize,
    p0: V3,
    u: V3,
    /// `h_delivered + layer_height`.
    t_max: f64,
    tip: Primitive,
    taper: Option<Primitive>,
    bbox: Aabb,
}

impl Zone {
    /// Is `p` in the zone, dilated by `margin`?
    ///
    /// Both halves of §3.6's definition are applied: the axial band, and
    /// membership of the contact's OWN tip group. The membership half is what
    /// stops a fat arm from being exempted merely for passing near the axis,
    /// and it subsumes the specification's radial `r_axis + d2` bound, which is
    /// the same set written as a radius rather than as a distance to the solid
    /// (`d3/2 = 2.7645e-2 mm > d2 = 2.3601e-2 mm`, so the dilation is the
    /// looser of the two and it is the one the sampling resolution demands).
    fn contains(&self, p: V3, margin: f64) -> bool {
        let d = v::sub(p, self.p0);
        let t = v::dot(d, self.u);
        if t < -margin || t > self.t_max + margin {
            return false;
        }
        let mut sd = self.tip.signed_distance(p);
        if let Some(tp) = self.taper {
            sd = sd.min(tp.signed_distance(p));
        }
        sd <= margin
    }
}

// ---------------------------------------------------------------------------
// Voxel accumulation
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
struct VoxAcc {
    count: u64,
    sum: V3,
    /// Extremes of the voxel's station along the attributed record's own
    /// centreline. Their difference is the **buried run** the prior probe
    /// reported as 0.586 mm at worst, measured here on cured voxels rather
    /// than on the centreline alone.
    t_lo: f64,
    t_hi: f64,
    /// Bounding box of the illegal material, so a solid with NO axis of its own
    /// (a junction ball) still reports how far its burial reaches.
    bbox: Aabb,
}

impl Default for VoxAcc {
    fn default() -> Self {
        Self {
            count: 0,
            sum: [0.0; 3],
            t_lo: f64::INFINITY,
            t_hi: f64::NEG_INFINITY,
            bbox: Aabb::empty(),
        }
    }
}

impl VoxAcc {
    fn add(&mut self, p: V3, t: f64) {
        self.count += 1;
        self.sum = v::add(self.sum, p);
        self.t_lo = self.t_lo.min(t);
        self.t_hi = self.t_hi.max(t);
        self.bbox.add_point(p);
    }
    /// Longest edge of the illegal material's bounding box, mm. Axis-free, so
    /// it is the number to compare against a probe that reported a "buried
    /// run" without saying which axis it was measured along.
    fn extent_mm(&self) -> f64 {
        if self.count == 0 || self.bbox.is_empty() {
            return 0.0;
        }
        let e = self.bbox.extent();
        e[0].max(e[1]).max(e[2])
    }
    fn centroid(&self) -> V3 {
        if self.count == 0 {
            return [0.0; 3];
        }
        v::mul(self.sum, 1.0 / self.count as f64)
    }
    /// Axial extent of the illegal material along its own record's centreline.
    fn axial_span_mm(&self) -> f64 {
        if self.count == 0 || !self.t_lo.is_finite() || !self.t_hi.is_finite() {
            return 0.0;
        }
        (self.t_hi - self.t_lo).max(0.0)
    }
}

/// `-0.0` is the identity `f64`'s `Sum` folds from, so an empty sum prints as
/// `-0`. Normalise it: a signed zero in a findings document is noise.
fn nz(x: f64) -> f64 {
    x + 0.0
}

fn voxel_centre(g: &RasterGrid, k: i64, row: i64, col: i64) -> V3 {
    let (x0, x1) = g.column_x_mm(col);
    let (y0, y1) = g.row_y_mm(row);
    [0.5 * (x0 + x1), 0.5 * (y0 + y1), g.layer_z_mm(k)]
}

// ---------------------------------------------------------------------------
// A bbox spatial hash, so the per-voxel tests are O(1) in the contact count
// ---------------------------------------------------------------------------

struct SpatialHash {
    cells: BTreeMap<[i64; 3], Vec<usize>>,
    /// Boxes too large to bucket without flooding the map. **They are never
    /// dropped** -- they are returned as a candidate for every query, which
    /// costs time and cannot cost an answer. Silently omitting one would make
    /// its own material look unattributed, or make a seat zone vanish, and
    /// both would be a wrong finding rather than a slow one.
    oversized: Vec<usize>,
    empty: Vec<usize>,
}

/// Cell budget for one box. `16^3` cells at a 2 mm cell is a 32 mm cube; no
/// corpus primitive or seat zone approaches it, so this only guards against a
/// corrupt record with an absurd bounding box.
const MAX_CELLS_PER_BOX: i64 = 4096;

impl SpatialHash {
    fn build(boxes: impl Iterator<Item = Aabb>) -> Self {
        let mut cells: BTreeMap<[i64; 3], Vec<usize>> = BTreeMap::new();
        let mut oversized: Vec<usize> = Vec::new();
        for (i, bb) in boxes.enumerate() {
            if bb.is_empty() {
                continue;
            }
            let lo = key(bb.min);
            let hi = key(bb.max);
            let span = (hi[0] - lo[0] + 1)
                .saturating_mul(hi[1] - lo[1] + 1)
                .saturating_mul(hi[2] - lo[2] + 1);
            if span > MAX_CELLS_PER_BOX {
                oversized.push(i);
                continue;
            }
            for x in lo[0]..=hi[0] {
                for y in lo[1]..=hi[1] {
                    for z in lo[2]..=hi[2] {
                        cells.entry([x, y, z]).or_default().push(i);
                    }
                }
            }
        }
        Self {
            cells,
            oversized,
            empty: Vec::new(),
        }
    }

    fn candidates(&self, p: V3) -> impl Iterator<Item = usize> + '_ {
        self.cells
            .get(&key(p))
            .map_or(&self.empty[..], |v| &v[..])
            .iter()
            .copied()
            .chain(self.oversized.iter().copied())
    }
}

fn key(p: V3) -> [i64; 3] {
    [
        (p[0] / HASH_CELL_MM).floor() as i64,
        (p[1] / HASH_CELL_MM).floor() as i64,
        (p[2] / HASH_CELL_MM).floor() as i64,
    ]
}

fn nearest_contact(points: &[(usize, V3)], p: V3) -> (Option<usize>, f64) {
    let mut best: Option<(usize, f64)> = None;
    for &(id, q) in points {
        let d = v::dist(p, q);
        if best.is_none_or(|(_, bd)| d < bd) {
            best = Some((id, d));
        }
    }
    best.map_or((None, f64::INFINITY), |(i, d)| (Some(i), d))
}

// ---------------------------------------------------------------------------
// Raster helpers -- local, because the shared ones walk the WHOLE model raster
// ---------------------------------------------------------------------------

/// `a` minus `b`, walking only `a`'s own runs.
///
/// `RunRaster::difference` is O(layers of both) and the model raster carries
/// thousands of them, so calling it once per contact would be quadratic in the
/// model. This walks the tip group only.
fn subtract_raster(a: &RunRaster, b: &RunRaster) -> RunRaster {
    let mut out = RunRaster::empty(a.grid);
    let mut pixel_count = 0u64;
    let mut run_count = 0u64;
    for layer in &a.layers {
        let bl = b.layer(layer.k);
        let mut rows: BTreeMap<i64, Vec<Run>> = BTreeMap::new();
        for (&row, runs) in &layer.rows {
            let cuts = bl.map_or(&[][..], |l| l.runs_in_row(row));
            let kept = subtract_runs(runs, cuts);
            if !kept.is_empty() {
                run_count += kept.len() as u64;
                pixel_count += kept.iter().map(|r| r.len().max(0) as u64).sum::<u64>();
                rows.insert(row, kept);
            }
        }
        if !rows.is_empty() {
            out.layers.push(crate::raster::Layer {
                k: layer.k,
                z_mm: layer.z_mm,
                rows,
            });
        }
    }
    out.pixel_count = pixel_count;
    out.run_count = run_count;
    out
}

fn subtract_runs(a: &[Run], cuts: &[Run]) -> Vec<Run> {
    let mut out: Vec<Run> = a.to_vec();
    for cut in cuts {
        let mut next = Vec::with_capacity(out.len() + 1);
        for r in &out {
            if cut.c1 <= r.c0 || cut.c0 >= r.c1 {
                next.push(*r);
                continue;
            }
            if r.c0 < cut.c0 {
                next.push(Run {
                    c0: r.c0,
                    c1: cut.c0,
                });
            }
            if cut.c1 < r.c1 {
                next.push(Run {
                    c0: cut.c1,
                    c1: r.c1,
                });
            }
        }
        out = next;
        if out.is_empty() {
            break;
        }
    }
    out
}

#[derive(Clone, Debug, Default)]
struct Component {
    count: u64,
    sum: V3,
}

impl Component {
    fn centroid(&self) -> V3 {
        if self.count == 0 {
            return [0.0; 3];
        }
        v::mul(self.sum, 1.0 / self.count as f64)
    }
}

/// 26-connected components of a run raster.
///
/// Deviation 3 of the module header: two voxels meeting along an edge or a
/// corner are one cured piece, and splitting them would manufacture breaches
/// out of raster staircase artefacts.
fn components_26(r: &RunRaster, grid: &RasterGrid) -> Vec<Component> {
    let mut items: Vec<(i64, i64, Run)> = Vec::new();
    for layer in &r.layers {
        for (&row, runs) in &layer.rows {
            for run in runs {
                items.push((layer.k, row, *run));
            }
        }
    }
    let n = items.len();
    if n == 0 {
        return Vec::new();
    }
    let mut index: BTreeMap<(i64, i64), Vec<usize>> = BTreeMap::new();
    for (i, it) in items.iter().enumerate() {
        index.entry((it.0, it.1)).or_default().push(i);
    }
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    let union = |p: &mut Vec<usize>, a: usize, b: usize| {
        let (ra, rb) = (find(p, a), find(p, b));
        if ra != rb {
            p[ra] = rb;
        }
    };
    for i in 0..n {
        let (k, row, run) = items[i];
        for dk in 0..=1i64 {
            for dr in -1..=1i64 {
                if dk == 0 && dr <= 0 {
                    continue;
                }
                let Some(list) = index.get(&(k + dk, row + dr)) else {
                    continue;
                };
                for &j in list {
                    let o = items[j].2;
                    // 26-connectivity: column ranges may merely touch.
                    if run.c0 <= o.c1 && o.c0 <= run.c1 {
                        union(&mut parent, i, j);
                    }
                }
            }
        }
    }
    let mut groups: BTreeMap<usize, Component> = BTreeMap::new();
    for i in 0..n {
        let root = find(&mut parent, i);
        let (k, row, run) = items[i];
        let e = groups.entry(root).or_default();
        for col in run.c0..run.c1 {
            e.count += 1;
            e.sum = v::add(e.sum, voxel_centre(grid, k, row, col));
        }
    }
    groups.into_values().collect()
}

/// Which exterior component is the one the arm leaves by.
fn pick_primary(kept: &[&Component], anchor: Option<V3>) -> Option<usize> {
    if kept.is_empty() {
        return None;
    }
    if let Some(a) = anchor {
        let mut best = (0usize, f64::INFINITY);
        for (i, k) in kept.iter().enumerate() {
            let d = v::dist(a, k.centroid());
            if d < best.1 {
                best = (i, d);
            }
        }
        return Some(best.0);
    }
    kept.iter()
        .enumerate()
        .max_by(|a, b| a.1.count.cmp(&b.1.count))
        .map(|(i, _)| i)
}

// ---------------------------------------------------------------------------
// The buried-centreline diagnostic
// ---------------------------------------------------------------------------

/// Work cap for the exact ray-parity cross-check, in sample-triangle units.
///
/// Ray parity casts a ray per sample against the welded mesh; the cost scales
/// with samples times triangles (the spatial index reduces the constant, not
/// the shape). Measured on this corpus: the 5k-triangle fixtures land near
/// 1e7 and finish in under a second, while the 429k-triangle treant with 2585
/// records lands near 1e11 and does not finish in twenty minutes. Beyond the
/// cap the cross-check reports `budget_exceeded` rather than quietly not
/// running -- the RASTER leg still measures, and its erosion is disclosed.
const MAX_MESH_WORK: f64 = 4.0e9;

struct Buried {
    /// Longest contiguous inside-the-model run along each record's centreline,
    /// measured on the CURED model raster.
    per_record: Vec<f64>,
    /// The same, measured by exact ray parity against the welded model mesh.
    /// The raster form is eroded by full containment; this one is not, so the
    /// pair brackets the true value.
    per_record_mesh: Vec<f64>,
    records_with_any: usize,
    max_run_mm: f64,
    members_with_any: usize,
    members_total: usize,
    max_member_run_mm: f64,
    mesh_records_with_any: usize,
    mesh_max_run_mm: f64,
    mesh_indeterminate_samples: usize,
    mesh_samples: usize,
    mesh_work: f64,
    mesh_budget_exceeded: bool,
    n_records: usize,
}

impl Buried {
    fn fraction_of_records(&self) -> f64 {
        if self.n_records == 0 {
            0.0
        } else {
            self.records_with_any as f64 / self.n_records as f64
        }
    }

    fn to_json(&self, n_records: usize) -> Json {
        let mut o = Json::obj();
        o.set("records_total", Json::usize(n_records));
        o.set("step_mm", Json::num(CENTRELINE_STEP_MM));

        let mut raster = Json::obj();
        raster.set(
            "records_with_buried_centreline",
            Json::usize(self.records_with_any),
        );
        raster.set("fraction_of_records", Json::num(self.fraction_of_records()));
        raster.set("max_buried_run_mm", Json::num(self.max_run_mm));
        raster.set("members_total", Json::usize(self.members_total));
        raster.set(
            "members_with_buried_centreline",
            Json::usize(self.members_with_any),
        );
        raster.set(
            "fraction_of_members",
            Json::num(if self.members_total == 0 {
                0.0
            } else {
                self.members_with_any as f64 / self.members_total as f64
            }),
        );
        raster.set("max_member_buried_run_mm", Json::num(self.max_member_run_mm));
        raster.set(
            "note",
            Json::str(
                "the cured model raster is eroded about one pixel by full containment, so \
                 this leg UNDER-reports",
            ),
        );
        o.set("on_cured_raster", raster);

        let mut mesh = Json::obj();
        mesh.set("budget_exceeded", Json::Bool(self.mesh_budget_exceeded));
        mesh.set("samples", Json::usize(self.mesh_samples));
        mesh.set("work_sample_triangles", Json::num(self.mesh_work));
        mesh.set("work_budget", Json::num(MAX_MESH_WORK));
        mesh.set(
            "indeterminate_samples",
            Json::usize(self.mesh_indeterminate_samples),
        );
        mesh.set(
            "records_with_buried_centreline",
            Json::usize(self.mesh_records_with_any),
        );
        mesh.set("max_buried_run_mm", Json::num(self.mesh_max_run_mm));
        mesh.set(
            "note",
            Json::str(
                "exact ray parity against the welded mesh: NOT eroded, so it is the upper \
                 leg of the bracket. A ray-parity sample that could not be settled is \
                 counted, never folded into 'outside'",
            ),
        );
        o.set("on_welded_mesh", mesh);

        o.set(
            "note",
            Json::str(
                "a contact taper is buried by design where its tip is seated, and is included \
                 in the record totals; this whole block is DISCLOSED, never gated",
            ),
        );
        o
    }
}

fn buried_runs(ctx: &ModelContext, model_r: &RunRaster) -> Buried {
    let n = ctx.model.records.len();
    let mut per_record = vec![0.0f64; n];
    let mut per_record_mesh = vec![0.0f64; n];
    let mut records_with_any = 0usize;
    let mut max_run = 0.0f64;
    let mut members_with_any = 0usize;
    let mut members_total = 0usize;
    let mut max_member_run = 0.0f64;

    // Batch-first: build every sample point once, then query in one call.
    // A per-sample call into the mesh from inside the record loop would be the
    // scalar-hot-path defect this project treats as a bug.
    let mut points: Vec<V3> = Vec::new();
    let mut ranges: Vec<(usize, usize, usize, f64)> = Vec::new();

    for ri in 0..n {
        let is_member = ctx.model.records[ri].kind == RecordKind::Member;
        if is_member {
            members_total += 1;
        }
        let Ok(prim) = ctx.primitive(ri) else { continue };
        let (a, b) = prim.centreline();
        let len = v::dist(a, b);
        if len <= 0.0 {
            continue;
        }
        let steps = (len / CENTRELINE_STEP_MM).ceil().max(1.0) as usize;
        let step = len / steps as f64;
        let lo = points.len();
        let mut run = 0.0f64;
        let mut best = 0.0f64;
        for s in 0..=steps {
            let t = s as f64 / steps as f64;
            let p = v::add(a, v::mul(v::sub(b, a), t));
            points.push(p);
            if model_r.contains_point(p) {
                run += step;
                best = best.max(run);
            } else {
                run = 0.0;
            }
        }
        ranges.push((ri, lo, points.len(), step));
        per_record[ri] = best;
        if best > 0.0 {
            records_with_any += 1;
            max_run = max_run.max(best);
            if is_member {
                members_with_any += 1;
                max_member_run = max_member_run.max(best);
            }
        }
    }

    let mesh_samples = points.len();
    let mesh_work = mesh_samples as f64 * ctx.model_mesh.tri_count() as f64;
    let mesh_budget_exceeded = mesh_work > MAX_MESH_WORK;
    let mut mesh_records_with_any = 0usize;
    let mut mesh_max_run = 0.0f64;
    let mut mesh_indeterminate = 0usize;
    if !mesh_budget_exceeded {
        let verdicts = ctx.model_mesh.points_inside(&points);
        for &(ri, lo, hi, step) in &ranges {
            let mut run = 0.0f64;
            let mut best = 0.0f64;
            for idx in lo..hi {
                match verdicts[idx] {
                    crate::mesh::Inside::Inside => {
                        run += step;
                        best = best.max(run);
                    }
                    crate::mesh::Inside::Outside => run = 0.0,
                    crate::mesh::Inside::Indeterminate => {
                        mesh_indeterminate += 1;
                        run = 0.0;
                    }
                }
            }
            per_record_mesh[ri] = best;
            if best > 0.0 {
                mesh_records_with_any += 1;
                mesh_max_run = mesh_max_run.max(best);
            }
        }
    }

    Buried {
        per_record,
        per_record_mesh,
        records_with_any,
        max_run_mm: max_run,
        members_with_any,
        members_total,
        max_member_run_mm: max_member_run,
        mesh_records_with_any,
        mesh_max_run_mm: mesh_max_run,
        mesh_indeterminate_samples: mesh_indeterminate,
        mesh_samples,
        mesh_work,
        mesh_budget_exceeded,
        n_records: n,
    }
}

// ---------------------------------------------------------------------------
// Failure-line plumbing
// ---------------------------------------------------------------------------

fn indeterminate_line(
    file: &str,
    model: usize,
    record: usize,
    reason: &'static str,
    detail: Option<&str>,
) -> Failure {
    Failure::line(
        "INTERSECT_INDETERMINATE",
        file,
        model,
        None,
        Some(record),
        &[("reason", reason.to_string())],
        {
            let mut o = Json::obj();
            o.set("reason", Json::str(reason));
            if let Some(d) = detail {
                o.set("detail", Json::str(d.to_string()));
            }
            o
        },
    )
}

/// Every item unestablished, because the solid the check grades does not exist.
fn all_indeterminate(
    mut out: CheckOutcome,
    file: &str,
    model: usize,
    n_records: usize,
    which: &str,
    err: &RasterError,
) -> CheckOutcome {
    let reason = match err {
        RasterError::BudgetExceeded { .. } => "raster_budget_exceeded",
        RasterError::MeshNotClosed | RasterError::EmptyOrNonFinite => "ray_parity_exhausted",
    };
    out.indeterminate(indeterminate_line(
        file,
        model,
        0,
        reason,
        Some(&format!("{which} raster: {err}")),
    ));
    out.indeterminate = n_records.max(1);
    let mut me = Json::obj();
    me.set("implemented", Json::Bool(true));
    me.set("raster", Json::str(which.to_string()));
    me.set("error", Json::str(err.to_string()));
    me.set(
        "headline",
        Json::str(format!(
            "the {which} raster could not be built ({err}); nothing established"
        )),
    );
    out.measured = me;
    out
}

/// Compact, unambiguous number text for a failure line.
fn num(x: f64) -> String {
    if !x.is_finite() {
        return format!("{x}");
    }
    if x == 0.0 {
        return "0".to_string();
    }
    if x.abs() < 1.0e-3 {
        return format!("{x:.3e}");
    }
    let s = format!("{x:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    if s.is_empty() {
        "0".to_string()
    } else {
        s
    }
}

fn v3(p: V3) -> String {
    format!("{},{},{}", num(p[0]), num(p[1]), num(p[2]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::Primitive;
    use crate::raster::{Budget, RasterGrid};

    fn grid() -> RasterGrid {
        RasterGrid::saturn4(0.0)
    }

    fn ball(centre: V3, r: f64) -> Primitive {
        Primitive::Ball { centre, r }
    }

    // -- the zone predicate's boundary ------------------------------------

    #[test]
    fn the_zone_ends_one_layer_past_the_delivered_entry() {
        // A ball of radius 0.3 whose south pole is the origin, seated 0.1 mm.
        let p0 = [0.0, 0.0, 0.0];
        let z = Zone {
            contact: 0,
            p0,
            u: [0.0, 0.0, 1.0],
            t_max: 0.1 + 0.05,
            tip: ball([0.0, 0.0, 0.3], 0.3),
            taper: None,
            bbox: ball([0.0, 0.0, 0.3], 0.3).aabb(),
        };
        let margin = 0.0;
        // On the axis, inside the band.
        assert!(z.contains([0.0, 0.0, 0.10], margin));
        // Exactly at the band's far edge.
        assert!(z.contains([0.0, 0.0, 0.15], margin));
        // One micron past it: refused. The band, not the ball, is the bound.
        assert!(!z.contains([0.0, 0.0, 0.150_001], margin));
        // Behind the south pole: refused.
        assert!(!z.contains([0.0, 0.0, -1.0e-6], margin));
    }

    #[test]
    fn the_zone_refuses_a_point_inside_the_band_but_outside_the_tip_group() {
        let p0 = [0.0, 0.0, 0.0];
        let z = Zone {
            contact: 0,
            p0,
            u: [0.0, 0.0, 1.0],
            t_max: 0.15,
            tip: ball([0.0, 0.0, 0.3], 0.3),
            taper: None,
            bbox: ball([0.0, 0.0, 0.3], 0.3).aabb(),
        };
        // At z = 0.1 the ball's own radius is sqrt(2*0.3*0.1 - 0.01) = 0.2236.
        let r_axis = (2.0f64 * 0.3 * 0.1 - 0.01).sqrt();
        assert!(z.contains([r_axis - 1e-6, 0.0, 0.1], 0.0));
        // A fat arm crossing the same band at 0.4 mm off the axis is NOT
        // exempt: the zone cannot be widened by a neighbouring solid.
        assert!(!z.contains([0.4, 0.0, 0.1], 0.0));
    }

    #[test]
    fn the_dilation_is_exactly_the_sampling_resolution_and_no_more() {
        let z = Zone {
            contact: 0,
            p0: [0.0, 0.0, 0.0],
            u: [0.0, 0.0, 1.0],
            t_max: 0.15,
            tip: ball([0.0, 0.0, 0.3], 0.3),
            taper: None,
            bbox: ball([0.0, 0.0, 0.3], 0.3).aabb(),
        };
        let m = c::D3_MM * 0.5;
        // The ball is centred at z = 0.3 with radius 0.3, so on the plane
        // z = 0.15 the surface sits at x = sqrt(0.3^2 - 0.15^2) and the
        // dilated surface at x = sqrt((0.3 + m)^2 - 0.15^2). A point just
        // inside the dilated surface is exempt; one just outside is not.
        let x_dilated = ((0.3 + m) * (0.3 + m) - 0.15 * 0.15).sqrt();
        assert!(z.contains([x_dilated - 1e-9, 0.0, 0.15], m));
        assert!(!z.contains([x_dilated + 1e-6, 0.0, 0.15], m));
        assert!(m > c::D2_MM, "d3/2 subsumes the specification's radial d2");
    }

    // -- clause (b): the far-side test ------------------------------------

    /// A slab of material with a ball driven through it leaves TWO exterior
    /// blobs; the same ball seated in a thick slab leaves ONE.
    fn slab_raster(thickness: f64) -> RunRaster {
        // A 4 x 4 mm slab centred on z = 0, built as a wide flat frustum.
        let p = Primitive::Frustum {
            a: [0.0, 0.0, -thickness * 0.5],
            ra: 2.0,
            b: [0.0, 0.0, thickness * 0.5],
            rb: 2.0,
        };
        RunRaster::from_primitives(&[p], grid(), &Budget::default()).expect("slab")
    }

    #[test]
    fn a_ball_through_a_thin_wall_leaves_two_exterior_components() {
        let g = grid();
        // Wall 0.20 mm thick; ball radius 0.30 mm centred in it, so it exits
        // both faces.
        let wall = slab_raster(0.20);
        let tip = ball([0.0, 0.0, 0.0], 0.30);
        let group = RunRaster::from_primitives(&[tip], g, &Budget::default()).expect("tip");
        let ext = subtract_raster(&group, &wall);
        let comps = components_26(&ext, &g);
        let voxel = g.voxel_volume_mm3();
        let kept = comps
            .iter()
            .filter(|k| k.count as f64 * voxel > voxel)
            .count();
        assert_eq!(kept, 2, "a punch-through leaves a second blob: {comps:?}");
    }

    #[test]
    fn a_ball_seated_in_a_thick_wall_leaves_one_exterior_component() {
        let g = grid();
        // Wall 4 mm thick, ball 0.30 mm sitting with its south pole 0.10 mm
        // below the top face: it cannot reach the far side.
        let wall = slab_raster(4.0);
        let tip = ball([0.0, 0.0, 2.0 - 0.10 + 0.30], 0.30);
        let group = RunRaster::from_primitives(&[tip], g, &Budget::default()).expect("tip");
        let ext = subtract_raster(&group, &wall);
        let comps = components_26(&ext, &g);
        let voxel = g.voxel_volume_mm3();
        let kept = comps
            .iter()
            .filter(|k| k.count as f64 * voxel > voxel)
            .count();
        assert_eq!(kept, 1, "no breach: {comps:?}");
    }

    // -- the unmeasurable input ------------------------------------------

    #[test]
    fn an_unrendered_tip_group_is_indeterminate_not_a_pass() {
        // A tip radius far below one pixel cures NOTHING under full
        // containment, so clause (b) has nothing to count. That is the
        // unmeasurable case, and it must not read as "one component, pass".
        let g = grid();
        let tiny = ball([0.0, 0.0, 1.0], 0.004);
        let group = RunRaster::from_primitives(&[tiny], g, &Budget::default()).expect("tiny");
        assert_eq!(group.pixel_count, 0, "sub-pixel geometry cures nothing");
        let comps = components_26(&group, &g);
        assert!(comps.is_empty());
        // The check maps that to INTERSECT_INDETERMINATE, and CheckOutcome
        // forces the invariant to fail.
        let mut o = CheckOutcome::new(1);
        o.indeterminate(indeterminate_line(
            "f",
            0,
            0,
            "tip_group_unrendered",
            Some("the tip group cures no voxel under full containment"),
        ));
        assert_eq!(o.status(), crate::check::Status::Fail);
        assert_eq!(o.indeterminate, 1);
    }

    #[test]
    fn an_empty_component_set_is_zero_components_not_one() {
        // The boundary that matters: "no exterior material" must never be
        // rounded up to the single component clause (b) wants.
        let g = grid();
        let empty = RunRaster::empty(g);
        assert_eq!(components_26(&empty, &g).len(), 0);
    }

    // -- the raster helpers ------------------------------------------------

    #[test]
    fn the_local_subtraction_agrees_with_the_shared_one() {
        let g = grid();
        let a = ball([0.0, 0.0, 1.0], 0.5);
        let b = ball([0.4, 0.0, 1.0], 0.5);
        let ra = RunRaster::from_primitives(&[a], g, &Budget::default()).expect("a");
        let rb = RunRaster::from_primitives(&[b], g, &Budget::default()).expect("b");
        let mine = subtract_raster(&ra, &rb);
        let theirs = ra.difference(&rb);
        assert_eq!(mine.pixel_count, theirs.pixel_count);
    }

    #[test]
    fn two_separated_blobs_are_two_components_and_a_touching_pair_is_one() {
        let g = grid();
        let far = RunRaster::from_primitives(
            &[ball([0.0, 0.0, 1.0], 0.3), ball([5.0, 0.0, 1.0], 0.3)],
            g,
            &Budget::default(),
        )
        .expect("two");
        assert_eq!(components_26(&far, &g).len(), 2);
        let near = RunRaster::from_primitives(
            &[ball([0.0, 0.0, 1.0], 0.3), ball([0.5, 0.0, 1.0], 0.3)],
            g,
            &Budget::default(),
        )
        .expect("overlapping");
        assert_eq!(components_26(&near, &g).len(), 1);
    }

    // -- end to end, through Check::run on a parsed file --------------------
    //
    // A helper that only exercises `components_26` proves the helper works,
    // not that the invariant fires. These two build a real `.chitubox` byte
    // image, parse it with the real reader, and run the real `Check`.

    use crate::context::{AuditContext, AuditOptions};
    use crate::reader::ChituboxFile;
    use crate::synth::{box_soup, SynthFile, SynthModel, SynthRecord};

    /// A tip group driven through a wall of `wall_half` half-thickness, with
    /// the ball's deepest point `p0_z` and the arm running down to a raft.
    fn punched_file(wall_half: f64, p0_z: f64) -> Vec<u8> {
        let r_ball = 0.3;
        let p0 = [0.0, 0.0, p0_z];
        let end = [0.0, 0.0, p0_z - 2.0 * r_ball];
        let centre = [0.0, 0.0, p0_z - r_ball];
        let taper_end = [0.0, 0.0, centre[2] - 0.9];
        let member_end = [0.0, 0.0, taper_end[2] - 3.0];
        let flare_end = [0.0, 0.0, member_end[2] - 0.9];
        let pad = [0.0, 0.0, flare_end[2] - 0.2];
        let model = SynthModel {
            name: "wall".into(),
            // z_plate = mesh.bbox.min.z - translate_z, so the lift puts the
            // raft exactly on the plate and no solid reaches below it.
            translate_mm: [0.0, 0.0, -wall_half - pad[2]],
            mesh: box_soup([-2.0, -2.0, -wall_half], [2.0, 2.0, wall_half]),
            records: vec![
                SynthRecord::new(RecordKind::ContactSphere, p0, end, r_ball, r_ball)
                    .with_matching_soup(),
                SynthRecord::new(RecordKind::ContactTaper, centre, taper_end, 0.2, 0.35)
                    .with_declared_depth(0.3)
                    .with_matching_soup(),
                SynthRecord::new(RecordKind::Member, taper_end, member_end, 0.35, 0.35)
                    .with_matching_soup(),
                SynthRecord::new(RecordKind::RootFlare, member_end, flare_end, 0.35, 1.1)
                    .with_matching_soup(),
                SynthRecord::new(RecordKind::RaftPadOrBridge, pad, pad, 2.2, 0.2).as_pad(),
            ],
        };
        SynthFile::new().with_model(model).to_bytes()
    }

    fn run_on(bytes: &[u8]) -> CheckOutcome {
        let file = ChituboxFile::parse(std::path::Path::new("/synthetic.chitubox"), bytes)
            .expect("the synthetic image parses");
        let options = AuditOptions::default();
        let audit = AuditContext {
            file: &file,
            options: &options,
            sidecar: None,
            sidecar_absence: None,
        };
        let ctx = ModelContext::build(&audit, 0);
        assert!(
            ctx.model_mesh.closure.is_closed(),
            "the synthetic wall must be a closed solid or the raster is refused"
        );
        Intersection.run(&ctx)
    }

    #[test]
    fn a_tip_driven_clean_through_a_thin_wall_fires_the_far_side_clause() {
        // Wall 0.2 mm thick; the ball's deepest point sits 0.1 mm ABOVE the far
        // face, so the complete unclipped ball straddles the wall and leaves a
        // cap on the far side.
        let out = run_on(&punched_file(0.1, 0.2));
        let breach = out
            .failures
            .iter()
            .find(|f| f.code == "INTERSECT_FAR_SIDE_BREACH")
            .expect("the far-side clause must fire on a punched wall");
        let n = breach
            .values
            .get("exterior_components")
            .and_then(Json::as_f64)
            .expect("exterior_components");
        assert!(n >= 2.0, "two blobs outside the model, got {n}");
        let vol = breach
            .values
            .get("breach_volume_mm3")
            .and_then(Json::as_f64)
            .expect("breach_volume_mm3");
        assert!(vol > 0.0, "the failure must say by how much, got {vol}");
        assert_eq!(out.status(), crate::check::Status::Fail);
    }

    #[test]
    fn the_same_tip_seated_in_a_thick_wall_does_not_fire_the_far_side_clause() {
        // The control: 4 mm wall, the ball seated 0.1 mm inside the underside.
        // Same records, same radii, same code path -- only the thickness and
        // the seat differ, so a breach here would be a false positive.
        let out = run_on(&punched_file(2.0, -1.9));
        assert!(
            !out.failures
                .iter()
                .any(|f| f.code == "INTERSECT_FAR_SIDE_BREACH"),
            "a seated tip is not a breach: {:?}",
            out.failures
                .iter()
                .map(|f| f.message.clone())
                .collect::<Vec<_>>()
        );
        let b = out
            .measured
            .get("clause_b")
            .expect("clause_b is always reported");
        assert_eq!(
            b.get("max_exterior_components").and_then(Json::as_f64),
            Some(1.0),
            "exactly one exterior component"
        );
    }

    #[test]
    fn a_model_with_no_triangles_is_indeterminate_not_a_vacuous_pass() {
        // The other unmeasurable input: supports present, model absent. An
        // empty mesh rasterises to an empty solid, so both clauses would pass
        // by vacuity. They must not.
        let mut bytes = punched_file(0.1, 0.2);
        // Rebuild the same file with an empty mesh rather than editing bytes.
        let file = ChituboxFile::parse(std::path::Path::new("/x.chitubox"), &bytes)
            .expect("baseline parses");
        assert!(!file.models[0].records.is_empty());
        bytes = SynthFile::new()
            .with_model(SynthModel {
                name: "no mesh".into(),
                translate_mm: [0.0, 0.0, 0.0],
                mesh: Vec::new(),
                records: vec![SynthRecord::new(
                    RecordKind::ContactSphere,
                    [0.0, 0.0, 0.3],
                    [0.0, 0.0, -0.3],
                    0.3,
                    0.3,
                )
                .with_matching_soup()],
            })
            .to_bytes();
        let out = run_on_allowing_open_mesh(&bytes);
        assert_eq!(out.status(), crate::check::Status::Fail);
        assert!(out.indeterminate > 0, "nothing was established");
        assert!(
            out.failures
                .iter()
                .any(|f| f.code == "INTERSECT_INDETERMINATE"),
            "and it says so"
        );
    }

    fn run_on_allowing_open_mesh(bytes: &[u8]) -> CheckOutcome {
        let file = ChituboxFile::parse(std::path::Path::new("/synthetic.chitubox"), bytes)
            .expect("the synthetic image parses");
        let options = AuditOptions::default();
        let audit = AuditContext {
            file: &file,
            options: &options,
            sidecar: None,
            sidecar_absence: None,
        };
        Intersection.run(&ModelContext::build(&audit, 0))
    }

    #[test]
    fn the_number_formatter_never_loses_the_sign_of_a_finding() {
        assert_eq!(num(0.0), "0");
        assert_eq!(num(1.33e-5), "1.330e-5");
        assert_eq!(num(0.586), "0.586");
        assert_eq!(v3([1.0, -2.5, 0.0]), "1,-2.5,0");
    }
}
