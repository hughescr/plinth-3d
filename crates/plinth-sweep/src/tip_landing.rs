//! The TIP-CONTACT SCAR: what the support field costs the model at the end
//! of an arm that CARRIES a down-facing surface — the mirror image of
//! [`crate::model_landing`], which prices the other end (where an arm
//! SEATS).
//!
//! # The gap this closes, and the one the seat scar left open
//!
//! `make_raking_blade` motivated both terms. Its two flip poses tie on the
//! analytic scar fold (65.42 mm² of fragility either way, because the
//! blade's broad faces are raked 85° — within 5° of vertical — and the
//! down-facing fold only counts faces steeper than the registered overhang
//! angle) and the seat scar landed in the commit before this one still could
//! not separate them on the damage the owner actually objects to: that
//! commit's own probe found the blade-down pose's one model seat was on the
//! blade, but INTERIOR (model material stands above it), while the visible
//! damage — arm TIPS rising from the plate and carrying the blade's whole
//! near-vertical down face — was priced at neither end. This module prices
//! that end.
//!
//! # What is measured
//!
//! For every arm the PRODUCTION sweep places, at its own `k_top` — the
//! narrow end, where the arm touches the surface it carries, not the seat
//! ([`crate::SweepStrut::tip_radius_mm`]):
//!
//! ```text
//! scar_mm2 = pi * r_tip^2 * fragility(carried face) * (1 + D(carried face))
//! ```
//!
//! summed over the arms that carry a face the analytic fold does not already
//! count (see "The double-counting choice" below), in mm². Exactly the
//! product [`crate::model_landing`] uses, so it folds into `scar_raw_mm2`
//! beside fragility, display and the landing scar under the same registered
//! `ORIENT_SCAR_WEIGHT`. **No constant, weight or exchange rate is
//! introduced by this module.**
//!
//! The demand cell an arm was placed for, at its own top layer, is exactly
//! [`crate::SweepStrut::cell_at`]`(strut.k_top)` — the up-sweep's own
//! bookkeeping, not a re-derivation: a leaned arm's cell at `k_top` is its
//! origin cell, and a straight arm's `cell_at` returns its one cell at every
//! layer. The slicing plane is the same `plate_z + (layer + 0.5) * height`
//! [`crate::model_landing`] uses, evaluated at `k_top`.
//!
//! # The double-counting choice
//!
//! The analytic fragility fold (`plinth_orient::score::score_one`) already
//! charges every down-facing face's WHOLE area × delicacy, as if the whole
//! face were scarred — it does not know a support field exists, so it prices
//! an assumption. The tip scar charges the ACTUAL footprints a real sweep
//! left. On a face the analytic fold already counts, both charges price the
//! same patch of the model: the fold prices the whole face on the
//! assumption every bit of it is scarred, and the tip term prices the real
//! contact circles that in fact touch it, which are always a subset. Adding
//! them is not two different kinds of damage, it is the same damage priced
//! twice at two different resolutions.
//!
//! **Chosen: charge a tip only against a face the analytic fold does NOT
//! already count** — a face whose down-facing test at this candidate's own
//! `down_limit` is false, i.e. `normal_z(face) >= down_limit`, the same
//! strict `<` `score_one` evaluates. [`TipCharge::already_counted`]
//! publishes the excluded flag per charge rather than silently dropping it,
//! so a reader can still recover the OTHER choice's number by summing
//! `scar_mm2` over every charge regardless of the flag.
//!
//! This was measured, not assumed, with `examples/tip_scar_probe.rs`, both
//! ways, on `make_raking_blade` and on Round 50 (the evidence README carries
//! the full run):
//!
//! | pose | tips | excluded (already counted) | choice(i) total | choice(ii) total |
//! |---|---|---|---|---|
//! | blade DOWN | 70 | 63 | 6.10239 mm² | 0.54968 mm² |
//! | blade UP | 67 | 56 | 5.32555 mm² | 0.38473 mm² |
//! | Round 50 flat (committed) | 243 | 210 | 21.53892 mm² | 0.02175 mm² |
//! | Round 50 180° flip | 283 | 250 | 17.70392 mm² | 0.05178 mm² |
//!
//! On Round 50, the overwhelming majority of tips land on the disc's own flat
//! underside, which is already fully counted by the analytic fold
//! (near-vertical-down normal, well past `down_limit`): folding ALL tip
//! charges in regardless of `already_counted` would move that disc's
//! `scar_raw_mm2` by ~21.5 mm², a real and visible number, for zero new
//! information — the fold already knows the disc's whole underside is
//! down-facing and delicate. Excluding those charges removes exactly that
//! inflation. **It does not reach exactly zero**, and that is not a defect in
//! the choice: `make_recessed_disc` has a genuine near-vertical wall (the
//! recess), which the fold's 45° cutoff does not count any more than it
//! counts the blade's raked faces, and a handful of tips land there in both
//! flip poses — 33 charges worth 0.02175 mm² and 0.05178 mm² respectively.
//! That is new information under the identical rule that makes the blade's
//! charges new information, not an inflation the choice should have removed.
//! On `make_raking_blade` the charge this module exists for — tips on the
//! blade's own raked faces — survives the exclusion in full: those faces are
//! raked 85°, five degrees off vertical, so `normal_z` sits far on the "not
//! already counted" side of `down_limit`. The exclusion removes ~99.9% of the
//! naive total on a flat base and preserves the whole signal on the fixture
//! this module was built for.
//!
//! # Interior seats, and why this module has no equivalent flag
//!
//! [`crate::model_landing`] discounts nothing for an interior SEAT — the
//! display field already prices visibility, per-face, and is already in the
//! product. A carried face is, by construction, a down-facing face with
//! nothing rooted below it at the layer the arm was placed: there is no
//! "interior tip" in the sense a seat can be interior, because a tip that
//! carried nothing would never have been placed. No second flag is needed
//! here.
//!
//! # Batching
//!
//! One collection pass over every strut, one parallel pass over the landing
//! set against every face (`landings × faces`, identical shape to
//! [`crate::model_landing::model_landing_scar`]), one serial sum. Nothing is
//! built or read per arm. Skipped outright when a pose places no arms.

use geom_kernel::{closest_on_tri, vec3};
use plinth_mesh::Mesh;
use rayon::prelude::*;

use crate::SweepPlan;

/// One arm's tip, in the frame the sweep planned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TipLanding {
    /// Index into [`SweepPlan::struts`].
    pub strut: usize,
    /// The centre of the cell the arm's axis occupies at its own `k_top`,
    /// at that layer's own slicing plane.
    pub point_mm: [f64; 3],
    /// The arm's own radius at its tip — the contact it leaves on the
    /// surface it carries ([`crate::SweepStrut::tip_radius_mm`]).
    pub radius_mm: f64,
}

/// One arm's tip, priced against the face it carries.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TipCharge {
    /// Index into [`SweepPlan::struts`].
    pub strut: usize,
    /// The nearest face of the oriented mesh to the tip point.
    pub face: usize,
    /// Distance from the tip point to that face. A tip sits ON the surface
    /// it carries, so this is a raster half-cell at most.
    pub distance_mm: f64,
    /// `π r_tip² × fragility × (1 + D)` of the carried face, computed
    /// regardless of [`Self::already_counted`] so the OTHER double-counting
    /// choice can be recovered by summing this field over every charge.
    pub scar_mm2: f64,
    /// Whether the carried face is already priced by the analytic
    /// down-facing fold at this candidate's own orientation
    /// (`normal_z(face) < down_limit`). A charge with this `true` is
    /// EXCLUDED from [`TipContactScar::raw_mm2`] — see the module doc's
    /// "double-counting choice".
    pub already_counted: bool,
}

/// What one pose's tip contacts cost the model.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TipContactScar {
    /// `Σ π r_tip² × fragility × (1 + D)` over charges NOT already counted by
    /// the analytic fold (the module doc's chosen half). Zero — exactly, and
    /// only — when the sweep placed no arms, or every carried face is
    /// already counted, or every carried face carries no fragility.
    pub raw_mm2: f64,
    /// Arms this pose placed.
    pub tips: usize,
    /// Of those, how many found a nearest face to be charged against. On a
    /// closed mesh, which the objective already requires, this equals
    /// `tips`.
    pub mapped: usize,
    /// Every tip that was charged, in strut order, with the face it was
    /// charged against and whether it was excluded as already counted.
    pub charges: Vec<TipCharge>,
}

/// Every arm of this plan, at its own tip, in strut order.
#[must_use]
pub fn tip_landings(plan: &SweepPlan) -> Vec<TipLanding> {
    let mut landings = Vec::with_capacity(plan.struts.len());
    for (index, strut) in plan.struts.iter().enumerate() {
        let cell = strut.cell_at(strut.k_top);
        if cell >= plan.grid.cells() {
            continue;
        }
        // The same slicing-plane derivation `model_landing::model_landings`
        // uses, evaluated at this arm's own `k_top` rather than its seat
        // layer.
        #[expect(
            clippy::cast_precision_loss,
            reason = "a layer index, bounded by the sweep's own layer count at ~1e4, is exact in f64"
        )]
        let z_mm = plan.plate_z_mm + (strut.k_top as f64 + 0.5) * plan.layer_height_mm;
        landings.push(TipLanding {
            strut: index,
            point_mm: [
                plan.grid.center_x_mm(plan.grid.col_of(cell)),
                plan.grid.center_y_mm(plan.grid.row_of(cell)),
                z_mm,
            ],
            radius_mm: strut.tip_radius_mm(),
        });
    }
    landings
}

/// The tip-contact scar of one finished plan, against the fields the
/// objective was scored with.
///
/// `mesh` must be the ORIENTED mesh this plan was planned on (see
/// [`crate::model_landing::model_landing_scar`]'s own doc for why), and
/// `down_limit` must be the same `-(OVERHANG_ANGLE_DEG.to_radians().sin())`
/// the objective's own down-facing fold evaluates, so "already counted" here
/// means exactly what it means there.
///
/// Returns `None` when there is no fragility field to weight a tip by —
/// the same stated-absence rule [`crate::model_landing::model_landing_scar`]
/// follows.
#[must_use]
pub fn tip_contact_scar(
    mesh: &Mesh,
    plan: &SweepPlan,
    fragility: &[f64],
    display: &[f64],
    down_limit: f64,
) -> Option<TipContactScar> {
    let faces = mesh.tris().len();
    if fragility.len() != faces {
        return None;
    }
    let landings = tip_landings(plan);
    if landings.is_empty() {
        return Some(TipContactScar {
            raw_mm2: 0.0,
            tips: 0,
            mapped: 0,
            charges: Vec::new(),
        });
    }

    // One read of the vertex triples AND the face normals for the whole
    // batch: the closest-point query needs the triangle, the down-facing
    // test needs its normal, and both are read here once rather than once
    // per landing.
    let verts = mesh.verts();
    let mut triangles: Vec<[[f64; 3]; 3]> = Vec::with_capacity(faces);
    let mut normal_z: Vec<f64> = Vec::with_capacity(faces);
    for triangle in mesh.tris() {
        let a = verts[triangle[0] as usize];
        let b = verts[triangle[1] as usize];
        let c = verts[triangle[2] as usize];
        triangles.push([a, b, c]);
        // The identical spelling `plinth_orient::score::score_one` builds its
        // own per-candidate normals with (cross, then divide by its own
        // norm) rather than `geom_kernel`'s reciprocal-multiply `normalize`,
        // so the "already counted" test agrees with the analytic fold to the
        // bit rather than to a ULP a boundary case could cross.
        let cross = vec3::cross(vec3::sub(b, a), vec3::sub(c, a));
        let length = vec3::norm(cross);
        // A degenerate face carries no area into the analytic fold either
        // (`score_one` skips it via its own zero-area guard), so it can
        // never be "already counted"; the placeholder normal is up-facing,
        // matching `score.rs`'s own degenerate placeholder.
        normal_z.push(if length.is_finite() && length > 0.0 { cross[2] / length } else { 1.0 });
    }

    let charged: Vec<Option<TipCharge>> = landings
        .par_iter()
        .map(|landing| {
            let mut best_distance = f64::INFINITY;
            let mut best_face: Option<usize> = None;
            for (face, triangle) in triangles.iter().enumerate() {
                let closest =
                    closest_on_tri(landing.point_mm, triangle[0], triangle[1], triangle[2]);
                let dx = landing.point_mm[0] - closest[0];
                let dy = landing.point_mm[1] - closest[1];
                let dz = landing.point_mm[2] - closest[2];
                let distance = dx * dx + dy * dy + dz * dz;
                if distance < best_distance {
                    best_distance = distance;
                    best_face = Some(face);
                }
            }
            best_face.map(|face| {
                let contact = core::f64::consts::PI * landing.radius_mm * landing.radius_mm;
                let fragile = contact * fragility[face];
                TipCharge {
                    strut: landing.strut,
                    face,
                    distance_mm: best_distance.sqrt(),
                    scar_mm2: fragile + fragile * display.get(face).copied().unwrap_or(0.0),
                    already_counted: normal_z[face] < down_limit,
                }
            })
        })
        .collect();

    let charges: Vec<TipCharge> = charged.into_iter().flatten().collect();
    let mut raw_mm2 = 0.0_f64;
    for charge in &charges {
        if !charge.already_counted {
            raw_mm2 += charge.scar_mm2;
        }
    }
    Some(TipContactScar {
        raw_mm2,
        tips: landings.len(),
        mapped: charges.len(),
        charges,
    })
}
