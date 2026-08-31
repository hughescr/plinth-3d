//! The shared load reconstruction of `audit-invariants.md` §2.
//!
//! I2, I3 and I4 all need "the model area this contact carries". The engine's
//! number for that is not in the file, so the audit builds its own, **once**,
//! and all three read the same product.
//!
//! **The demand set `D` (§2.1).** For a model triangle with outward unit
//! normal `n` and area `A`, the film-contact area it presents is its
//! projection onto the plate: `demand = A * |n . z_hat|` when `n . z < 0`, and
//! zero otherwise. Derived, not chosen: it is exactly the quantity the
//! calibration coupons measured, so `X_ALLOWED` is applied to the quantity it
//! was calibrated on. It degrades to zero for a vertical wall, which is
//! correct, and it needs no self-support angle, no reach radius and no
//! placeholder.
//!
//! **Ownership (§2.2)** is a COMPLETE Voronoi partition on each surface
//! component, with **no reach limit**. That is the project's own doctrine
//! (`docs/physics.md` §3): a contact standing alone under a large face
//! genuinely owns that face, and the area law is entitled to say so. Ties
//! within `tol_geom` resolve to the lower record index, so the partition is
//! deterministic.
//!
//! **Batch-first, and streaming.** Samples are never materialised. Subdividing
//! every down-facing triangle to a `d2 = 0.0236 mm` edge on the treant would
//! be tens of millions of rows; accumulating per contact in one pass costs the
//! same arithmetic and none of the memory. The sample budget is enforced and
//! exceeding it is `sample_budget_exceeded`, which FAILS -- it never truncates.

use crate::consts as c;
use crate::mesh::Mesh;
use crate::vec3::{self as v, V3};

/// Per-triangle demand, and the totals it sums to.
#[derive(Clone, Debug)]
pub struct DemandSet {
    /// `demand(t)` for every model triangle, mm^2. Zero for anything not
    /// down-facing.
    pub per_triangle_mm2: Vec<f64>,
    /// `D_total`, mm^2.
    pub total_mm2: f64,
    /// `D` restricted to each surface component, mm^2.
    pub per_component_mm2: Vec<f64>,
    /// True surface area of the down-facing set, mm^2 -- reported beside the
    /// projected total so a reader can see how much projection removed.
    pub down_face_true_area_mm2: f64,
    pub down_facing_triangles: usize,
}

impl DemandSet {
    /// Build the demand set from the welded model mesh.
    ///
    /// `outward_sign` is applied to the stored facet normals. Pass `1.0` for a
    /// mesh whose `Mesh::normals` are already outward, which
    /// [`Mesh::weld_exact`] arranges for a single positively-oriented shell.
    /// **On a mesh with cavity shells the caller must verify outwardness per
    /// shell first** -- see [`Mesh::normal_agrees_with_solid`] -- because an
    /// inward-wound cavity shell would contribute its ceiling to the wrong
    /// side of the `n . z < 0` test.
    #[must_use]
    pub fn build(m: &Mesh) -> Self {
        let n = m.tri_count();
        let mut per_triangle_mm2 = vec![0.0f64; n];
        let mut per_component_mm2 = vec![0.0f64; m.component_count];
        let mut total = 0.0f64;
        let mut true_area = 0.0f64;
        let mut count = 0usize;
        for ti in 0..n {
            let nz = m.normals[ti][2];
            if nz < 0.0 {
                let d = m.areas[ti] * nz.abs();
                per_triangle_mm2[ti] = d;
                per_component_mm2[m.component_of_tri[ti] as usize] += d;
                total += d;
                true_area += m.areas[ti];
                count += 1;
            }
        }
        Self {
            per_triangle_mm2,
            total_mm2: total,
            per_component_mm2,
            down_face_true_area_mm2: true_area,
            down_facing_triangles: count,
        }
    }
}

/// A contact, as the ownership partition needs it.
#[derive(Clone, Copy, Debug)]
pub struct ContactSite {
    /// The `contact_id`: the record index of the type-7 TIP record.
    pub contact_id: usize,
    /// `p0`, the ball's deepest point, which §2.2 measures distance from.
    pub p0: V3,
    /// The surface component this contact is routed to, from its seat entry
    /// triangle (§3.1). `None` means the routing failed and the contact owns
    /// nothing -- which is `component_routing_failed`, a FAILURE, never a pass.
    pub component: Option<usize>,
}

/// The result of the Voronoi partition.
#[derive(Clone, Debug)]
pub struct Ownership {
    /// `A_owned(c)` per entry of the `sites` slice passed in, mm^2.
    pub a_owned_mm2: Vec<f64>,
    /// Demand on components carrying no contact, mm^2. I4 clause (a).
    pub a_unowned_mm2: f64,
    /// Components carrying demand and no contact at all.
    pub components_without_contact: Vec<usize>,
    /// `(component, demand_mm2, centroid)` for the worst such component.
    pub largest_unowned_component: Option<(usize, f64, V3)>,
    /// Samples taken. Reported so the resolution behind a number is visible.
    pub samples: u64,
    /// Sum of `a_owned` plus `a_unowned`; I4 clause (b) compares it to
    /// `D_total`.
    pub accounted_mm2: f64,
    /// Set when the sample budget was hit. **A `Some` here forces failure**
    /// with `reason=sample_budget_exceeded`.
    pub budget_exceeded: Option<u64>,
}

/// Sampling limits. `max_samples` is deliberately generous but finite: the
/// specified `d2` edge cap on a 429k-face model can demand tens of millions of
/// samples, and exceeding the budget must be a reported failure rather than a
/// silent coarsening.
#[derive(Clone, Copy, Debug)]
pub struct SampleBudget {
    pub max_samples: u64,
    /// Longest edge a terminal sample triangle may have, mm. §2.1 pins this to
    /// the in-plane pixel diagonal `d2`, which ties the sampling resolution to
    /// the printer rather than to an analysis constant.
    pub max_edge_mm: f64,
    /// Guard against a pathological single triangle: subdivision depth cap.
    pub max_depth: u32,
}

impl Default for SampleBudget {
    fn default() -> Self {
        Self {
            // MEASURED 2026-08-30, integration sweep: the 40,000,000 this
            // default originally carried is exhausted by ordinary corpus
            // models. `exports/cylinder-first-artifact.chitubox` needs
            // 132,907,008 samples to complete the partition, 3.3x the old
            // default. Exhaustion is not a soft failure: `A_owned` becomes
            // undefined, and I2, I3, I4 AND I8 all read `A_owned`, so ONE
            // exhausted budget turned 47 of 54 contacts indeterminate in I2,
            // 54 of 54 in I3 and 360 of 363 records in I8 -- the audit
            // measured almost nothing on a real file and reported it as
            // failure-by-ignorance rather than as a finding about the plan.
            //
            // Raised to 1e9, which is still finite (the guard against a
            // pathological mesh is intact and exceeding it still FAILS rather
            // than coarsening) but clears every corpus model measured so far
            // with an order of magnitude of headroom. Sampling costs roughly
            // 0.45 us per sample, so 1e9 is about 450 s of one core -- inside
            // the 1500 s per-model bound this project works to.
            max_samples: 1_000_000_000,
            max_edge_mm: c::D2_MM,
            max_depth: 24,
        }
    }
}

/// Build the complete Voronoi partition of the demand set among the contacts.
///
/// Streaming: no sample is stored. One pass over the down-facing triangles,
/// each subdivided on an explicit stack, each terminal centroid charged to its
/// nearest same-component contact.
#[must_use]
pub fn partition(
    m: &Mesh,
    demand: &DemandSet,
    sites: &[ContactSite],
    budget: &SampleBudget,
) -> Ownership {
    let mut a_owned_mm2 = vec![0.0f64; sites.len()];
    let mut a_unowned_mm2 = 0.0f64;
    let mut samples: u64 = 0;
    let mut budget_exceeded = None;

    // Group contacts by surface component so the nearest-contact scan touches
    // only candidates that can legally own the sample.
    let mut by_component: Vec<Vec<usize>> = vec![Vec::new(); m.component_count];
    for (i, s) in sites.iter().enumerate() {
        if let Some(cmp) = s.component {
            if cmp < by_component.len() {
                by_component[cmp].push(i);
            }
        }
    }

    let mut unowned_by_component = vec![0.0f64; m.component_count];
    let mut centroid_acc: Vec<(V3, f64)> = vec![([0.0; 3], 0.0); m.component_count];

    let mut stack: Vec<([V3; 3], u32)> = Vec::with_capacity(64);
    for ti in 0..m.tri_count() {
        let d = demand.per_triangle_mm2[ti];
        if d <= 0.0 {
            continue;
        }
        let comp = m.component_of_tri[ti] as usize;
        let cands: &[usize] = by_component.get(comp).map_or(&[], Vec::as_slice);
        // Density: demand per unit true area is constant over one flat facet,
        // so a sub-triangle's share is its own area fraction.
        let facet_area = m.areas[ti];
        if facet_area <= 0.0 {
            continue;
        }
        let density = d / facet_area;

        stack.clear();
        stack.push((m.tri(ti), 0));
        while let Some((t, depth)) = stack.pop() {
            let e0 = v::dist(t[0], t[1]);
            let e1 = v::dist(t[1], t[2]);
            let e2 = v::dist(t[2], t[0]);
            let longest = e0.max(e1).max(e2);
            if longest > budget.max_edge_mm && depth < budget.max_depth {
                // Dyadic 1-to-4 subdivision, as §2.1 specifies.
                let m01 = v::midpoint(t[0], t[1]);
                let m12 = v::midpoint(t[1], t[2]);
                let m20 = v::midpoint(t[2], t[0]);
                stack.push(([t[0], m01, m20], depth + 1));
                stack.push(([m01, t[1], m12], depth + 1));
                stack.push(([m20, m12, t[2]], depth + 1));
                stack.push(([m01, m12, m20], depth + 1));
                continue;
            }
            samples += 1;
            if samples > budget.max_samples {
                budget_exceeded = Some(budget.max_samples);
                break;
            }
            let w = v::tri_area(t[0], t[1], t[2]) * density;
            let x = [
                (t[0][0] + t[1][0] + t[2][0]) / 3.0,
                (t[0][1] + t[1][1] + t[2][1]) / 3.0,
                (t[0][2] + t[1][2] + t[2][2]) / 3.0,
            ];
            if cands.is_empty() {
                a_unowned_mm2 += w;
                unowned_by_component[comp] += w;
                centroid_acc[comp].0 = v::add(centroid_acc[comp].0, v::mul(x, w));
                centroid_acc[comp].1 += w;
                continue;
            }
            // Nearest contact on the same surface component. Ties within
            // tol_geom go to the lower record index, so the answer does not
            // depend on iteration order.
            let mut best = cands[0];
            let mut best_d = v::dist(x, sites[cands[0]].p0);
            for &ci in &cands[1..] {
                let dd = v::dist(x, sites[ci].p0);
                let tol = c::tol_geom(x, sites[ci].p0);
                if dd < best_d - tol
                    || (dd <= best_d + tol && sites[ci].contact_id < sites[best].contact_id)
                {
                    best = ci;
                    best_d = best_d.min(dd);
                }
            }
            a_owned_mm2[best] += w;
        }
        if budget_exceeded.is_some() {
            break;
        }
    }

    let mut components_without_contact: Vec<usize> = Vec::new();
    let mut largest: Option<(usize, f64, V3)> = None;
    for cmp in 0..m.component_count {
        if unowned_by_component[cmp] > 0.0 {
            components_without_contact.push(cmp);
            let (acc, wsum) = centroid_acc[cmp];
            let centroid = if wsum > 0.0 {
                v::mul(acc, 1.0 / wsum)
            } else {
                [0.0; 3]
            };
            if largest.is_none_or(|(_, best, _)| unowned_by_component[cmp] > best) {
                largest = Some((cmp, unowned_by_component[cmp], centroid));
            }
        }
    }

    let accounted_mm2 = a_owned_mm2.iter().sum::<f64>() + a_unowned_mm2;
    Ownership {
        a_owned_mm2,
        a_unowned_mm2,
        components_without_contact,
        largest_unowned_component: largest,
        samples,
        accounted_mm2,
        budget_exceeded,
    }
}

/// The engine's own books, recovered by inversion (§2.3).
///
/// Measured 2026-08-30: `X = carried_area / (pi * r_tip^2) = 12.000` on every
/// contact, maximum deviation 7e-15. The engine sizes every tip exactly to the
/// law, so the tip radius in the file **is** the engine's declared carried
/// area.
///
/// **This is DECLARED, by inversion, and it stops being meaningful the moment
/// a future engine sizes tips any other way** (`audit-invariants.md` §6 item
/// 3). The inversion is injective only away from the sizing clamps, which is
/// what [`ImpliedArea::clamped`] records.
#[derive(Clone, Copy, Debug)]
pub struct ImpliedArea {
    pub contact_id: usize,
    pub r_tip_mm: f64,
    /// `x_allowed * pi * r_tip^2`, mm^2. Meaningful as a point value only when
    /// `clamped` is false.
    pub a_implied_mm2: f64,
    /// The tip sits on a sizing clamp, so the inversion is not injective and
    /// the value must enter conservation checks as an INTERVAL, never as a
    /// point and never as an exemption.
    pub clamped: bool,
}

/// Invert the area law for one tip.
#[must_use]
pub fn implied_area(contact_id: usize, r_tip_mm: f64, x_allowed: f64) -> ImpliedArea {
    let tol = c::tol_geom_mag(r_tip_mm);
    let clamped =
        r_tip_mm <= c::TIP_RADIUS_MIN_ALLOWED_MM + tol || r_tip_mm >= c::TIP_RADIUS_MAX_MM - tol;
    ImpliedArea {
        contact_id,
        r_tip_mm,
        a_implied_mm2: x_allowed * std::f64::consts::PI * r_tip_mm * r_tip_mm,
        clamped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::Soup;

    /// A flat 10x10 mm plate 1 mm thick, sitting with its underside at z = 0.
    fn slab_soup(w: f32, h: f32) -> Soup {
        let (a, b) = (w, h);
        let q = [
            [0.0, 0.0, 0.0],
            [a, 0.0, 0.0],
            [a, b, 0.0],
            [0.0, b, 0.0],
            [0.0, 0.0, 1.0],
            [a, 0.0, 1.0],
            [a, b, 1.0],
            [0.0, b, 1.0],
        ];
        let faces: [[usize; 3]; 12] = [
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [1, 2, 6],
            [1, 6, 5],
            [2, 3, 7],
            [2, 7, 6],
            [3, 0, 4],
            [3, 4, 7],
        ];
        Soup {
            tris: faces.iter().map(|f| [q[f[0]], q[f[1]], q[f[2]]]).collect(),
            source_offset: 0,
        }
    }

    #[test]
    fn demand_is_the_plate_projection_of_the_down_face_only() {
        let m = Mesh::weld_exact(&slab_soup(10.0, 10.0));
        let d = DemandSet::build(&m);
        // The underside is 10 x 10 flat, so projected == true == 100 mm^2.
        assert!((d.total_mm2 - 100.0).abs() < 1e-9, "{}", d.total_mm2);
        assert!((d.down_face_true_area_mm2 - 100.0).abs() < 1e-9);
        assert_eq!(d.down_facing_triangles, 2, "only the two bottom facets");
    }

    #[test]
    fn a_vertical_wall_presents_no_demand() {
        // A tall thin box: its four walls are vertical and contribute nothing.
        let m = Mesh::weld_exact(&slab_soup(0.5, 0.5));
        let d = DemandSet::build(&m);
        assert!((d.total_mm2 - 0.25).abs() < 1e-9, "{}", d.total_mm2);
    }

    #[test]
    fn one_contact_owns_the_whole_face_because_there_is_no_reach_limit() {
        let m = Mesh::weld_exact(&slab_soup(10.0, 10.0));
        let d = DemandSet::build(&m);
        let sites = [ContactSite {
            contact_id: 0,
            p0: [0.5, 0.5, 0.0],
            component: Some(0),
        }];
        let o = partition(&m, &d, &sites, &SampleBudget::default());
        assert!(o.budget_exceeded.is_none(), "{o:?}");
        assert!(o.a_unowned_mm2 == 0.0);
        assert!(
            (o.a_owned_mm2[0] - 100.0).abs() < 1e-6,
            "a contact in one corner owns the whole face: {}",
            o.a_owned_mm2[0]
        );
        // Clause (b): the partition identity holds.
        assert!((o.accounted_mm2 - d.total_mm2).abs() <= c::tol_sum(d.total_mm2));
    }

    #[test]
    fn two_contacts_split_a_symmetric_face_in_half() {
        let m = Mesh::weld_exact(&slab_soup(10.0, 10.0));
        let d = DemandSet::build(&m);
        let sites = [
            ContactSite {
                contact_id: 0,
                p0: [2.5, 5.0, 0.0],
                component: Some(0),
            },
            ContactSite {
                contact_id: 1,
                p0: [7.5, 5.0, 0.0],
                component: Some(0),
            },
        ];
        let o = partition(&m, &d, &sites, &SampleBudget::default());
        for a in &o.a_owned_mm2 {
            assert!((a - 50.0).abs() < 0.05, "{a}");
        }
        assert!((o.accounted_mm2 - 100.0).abs() < 1e-6);
    }

    #[test]
    fn a_face_with_no_contact_is_unowned_and_named() {
        let m = Mesh::weld_exact(&slab_soup(10.0, 10.0));
        let d = DemandSet::build(&m);
        let o = partition(&m, &d, &[], &SampleBudget::default());
        assert!((o.a_unowned_mm2 - 100.0).abs() < 1e-6);
        assert_eq!(o.components_without_contact, vec![0]);
        let (cmp, area, centroid) = o.largest_unowned_component.expect("named");
        assert_eq!(cmp, 0);
        assert!((area - 100.0).abs() < 1e-6);
        assert!((centroid[0] - 5.0).abs() < 0.05 && (centroid[1] - 5.0).abs() < 0.05);
    }

    #[test]
    fn the_sample_budget_is_reported_never_absorbed() {
        let m = Mesh::weld_exact(&slab_soup(10.0, 10.0));
        let d = DemandSet::build(&m);
        let sites = [ContactSite {
            contact_id: 0,
            p0: [5.0, 5.0, 0.0],
            component: Some(0),
        }];
        let o = partition(
            &m,
            &d,
            &sites,
            &SampleBudget {
                max_samples: 100,
                ..SampleBudget::default()
            },
        );
        assert_eq!(o.budget_exceeded, Some(100));
    }

    #[test]
    fn inverting_the_area_law_recovers_the_engines_own_books() {
        // X = carried / (pi r^2) = 12 exactly, so the inverse is exact too.
        let a = implied_area(0, 0.197, c::X_ALLOWED);
        let want = 12.0 * std::f64::consts::PI * 0.197 * 0.197;
        assert!((a.a_implied_mm2 - want).abs() < 1e-15);
        assert!(!a.clamped, "the median tip is not on a clamp");
        assert!(implied_area(1, c::TIP_RADIUS_MIN_ALLOWED_MM, c::X_ALLOWED).clamped);
        assert!(implied_area(2, c::TIP_RADIUS_MAX_MM, c::X_ALLOWED).clamped);
    }
}
