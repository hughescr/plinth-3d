//! The MODEL-LANDING SCAR: what the support field costs the model at the end
//! of an arm that seats on the model itself.
//!
//! # The gap this closes
//!
//! The orientation objective's scar term is `Σ_down(area × (fragility +
//! D))` — a measurement of the model's own down-facing area, and only of
//! that. It cannot see a support field, so it cannot see a support field
//! landing on the one delicate thing on the model.
//!
//! `make_raking_blade` is the fixture that shows it. A 20×20×4 mm slab with a
//! 0.03 mm blade raked 85° below it: the two flip poses have IDENTICAL support
//! area, height, peel force and fragility (65.42 mm² of scar in both), because
//! the blade's broad faces are within 5° of vertical and the down-facing fold
//! only counts faces steeper than the registered overhang angle. The measured
//! burden then decided the pose — 67 arms against 70 — and the winner was the
//! pose that seats a strut ON THE BLADE. Nothing in the objective saw that.
//!
//! # What is measured, and under whose weight
//!
//! For every arm the PRODUCTION sweep lands on the model
//! ([`crate::Landing::Model`]):
//!
//! ```text
//! scar_mm2 = pi * r_seat^2 * (fragility(landed face) + D(landed face))
//! ```
//!
//! summed over the arms, in mm². That is the identical product the down-facing
//! fold uses — an area in mm², times the delicacy of the surface it is charged
//! against PLUS its display value, two independent harms of one contact that
//! add (2026-09-29, `plinth_orient::score`'s module doc) — so it is folded into
//! `scar_raw_mm2` beside the other two halves and priced by the SAME registered
//! `ORIENT_SCAR_WEIGHT`. **No constant, weight or exchange rate is introduced
//! by this module.** Every number it reads is already measured elsewhere: the
//! seat radii come from the sweep's own sizing, the fragility and display
//! fields come from the `OrientFaceTable` the objective was scored against, and
//! the geometry comes from the same oriented mesh the sweep planned on.
//!
//! ## `r_seat` is the radius where the arm LANDS, not `tip_radius_mm()`
//!
//! [`crate::SweepStrut::tip_radius_mm`] is the radius at `k_top` — the narrow
//! end, where the arm touches the surface it CARRIES. That end's scar is
//! already priced: the surface it touches is a down-facing surface and its area
//! is in the down-facing fold. The end this module prices is the other one, at
//! `k_bottom`, where the arm seats on the model, and the contact it leaves
//! there is its own section at that layer — `radius_mm[0]`, the widest, because
//! the area law grows an arm downward as load accumulates. Charging the tip
//! radius here would under-state the real contact by the whole growth of the
//! arm.
//!
//! ## Interior landings are counted, not discounted
//!
//! An arm whose seat has model material above it in the same column is flagged
//! [`crate::SweepStrut::interior`], and the owner's 2026-08-31 ruling is that
//! such a scar is never seen and never removed. This module reports how many
//! landings were interior ([`ModelLandingScar::interior`]) and charges them
//! anyway: the display field `D` is the quantity that already prices
//! visibility, it is measured per face and it is in the product, so discounting
//! interior seats a second time here would be pricing the same thing twice with
//! a rule instead of a measurement. The count is published so the choice is
//! visible rather than implied.
//!
//! # Finding the landed face
//!
//! The sweep knows a landing as a raster CELL and a LAYER; the fragility and
//! display fields are per FACE. The bridge is the nearest face of the same
//! oriented mesh the sweep sliced, to the seat point (cell centre, layer
//! plane) — the exact nearest, by the workspace's canonical
//! [`geom_kernel::closest_on_tri`], never a private copy of point-vs-triangle
//! distance.
//!
//! It is BATCHED as one pass: every landing of one pose is collected first, the
//! per-face vertex triples are read once, and the whole landing set is answered
//! in one parallel scan. Nothing is built or read per arm. The scan is
//! `landings × faces`, which is why it is skipped outright when a pose has no
//! model landings — the case of every flat disc in the acceptance set, where
//! this module must cost nothing and change nothing.

use geom_kernel::closest_on_tri;
use plinth_mesh::Mesh;
use rayon::prelude::*;

use crate::{Landing, SweepPlan};

/// One arm's seat on the model, in the frame the sweep planned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelLanding {
    /// Index into [`SweepPlan::struts`].
    pub strut: usize,
    /// The seat point: the centre of the cell the arm's axis occupies where it
    /// lands, at that landing layer's own slicing plane.
    pub point_mm: [f64; 3],
    /// The arm's own radius at its seat — the contact it leaves on the model.
    pub radius_mm: f64,
    /// Model material stands above this seat in the same column.
    pub interior: bool,
}

/// One arm's seat, priced against the face it was charged to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LandingCharge {
    /// Index into [`SweepPlan::struts`].
    pub strut: usize,
    /// The nearest face of the oriented mesh to the seat — the face whose
    /// fragility and display value priced this seat.
    pub face: usize,
    /// Distance from the seat point to that face. A seat sits ON the model, so
    /// this is a raster half-cell at most; a large value means the seat was
    /// charged to a face it does not touch and is a reason to look.
    pub distance_mm: f64,
    /// `π r_seat² × (fragility + D)` of that face.
    pub scar_mm2: f64,
    pub interior: bool,
}

/// What one pose's model landings cost the model.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelLandingScar {
    /// `Σ π r_seat² × (fragility + D)` in mm². Zero — exactly, and only —
    /// when nothing landed on the model, or when every landed face carries
    /// neither fragility nor display value.
    pub raw_mm2: f64,
    /// Arms that seated on the model.
    pub landings: usize,
    /// Of those, how many found a nearest face to be charged against. A
    /// landing that finds none is DISCLOSED here rather than silently costing
    /// nothing; on a closed mesh, which the objective already requires, this
    /// equals `landings`.
    pub mapped: usize,
    /// Of those, how many had model material above them (see the module doc).
    pub interior: usize,
    /// Every seat that was charged, in strut order, with the face it was
    /// charged against.
    ///
    /// Published because "the term is non-zero" is not the claim worth
    /// checking — WHICH face a seat was charged to is. A test that only reads
    /// `raw_mm2` cannot tell a seat on the one delicate feature from a seat on
    /// a plain slab, and neither can a reader of an evidence file.
    pub charges: Vec<LandingCharge>,
}

/// Every arm of this plan that seats on the model, in strut order.
///
/// The cell is the arm's own axis cell AT ITS LANDING LAYER
/// ([`crate::SweepStrut::cell_at`]), so a leaned arm is charged where it
/// actually seats rather than where it started.
#[must_use]
pub fn model_landings(plan: &SweepPlan) -> Vec<ModelLanding> {
    let mut landings = Vec::new();
    for (index, strut) in plan.struts.iter().enumerate() {
        let Landing::Model { layer } = strut.landing else {
            continue;
        };
        let cell = strut.cell_at(layer);
        if cell >= plan.grid.cells() {
            continue;
        }
        // The slicing plane of a layer, exactly as `run_sweep` builds its
        // `planes`: `plate_z + (layer + 0.5) * height`. Re-deriving it here
        // from the plan's own published plate and layer height keeps the two
        // from being able to disagree.
        #[expect(
            clippy::cast_precision_loss,
            reason = "a layer index, bounded by the sweep's own layer count at ~1e4, is exact in f64"
        )]
        let z_mm = plan.plate_z_mm + (layer as f64 + 0.5) * plan.layer_height_mm;
        landings.push(ModelLanding {
            strut: index,
            point_mm: [
                plan.grid.center_x_mm(plan.grid.col_of(cell)),
                plan.grid.center_y_mm(plan.grid.row_of(cell)),
                z_mm,
            ],
            // The seat radius: the first entry is the layer `k_bottom`, which
            // is where the arm lands. An arm the down-sweep never sized has an
            // empty ledger and no contact to charge.
            radius_mm: strut.radius_mm.first().copied().unwrap_or(0.0),
            interior: strut.interior,
        });
    }
    landings
}

/// The model-landing scar of one finished plan, against the fields the
/// objective was scored with.
///
/// `mesh` must be the ORIENTED mesh this plan was planned on, and `fragility`
/// and `display` must be in that mesh's face order — which is the authored
/// mesh's face order, since orienting rotates vertices and never reorders
/// triangles.
///
/// Returns `None` when there is no fragility field to weight a seat by: a
/// stated absence, the same rule `OrientFaceTable::display` follows, and never
/// a zero. `Some` with `raw_mm2 == 0.0` is the positive assertion that a real
/// sweep ran and its arms cost the model nothing.
///
/// An empty `display` is read as `D = 0`, so a table built
/// without a display field reproduces `π r² × fragility` exactly.
#[must_use]
pub fn model_landing_scar(
    mesh: &Mesh,
    plan: &SweepPlan,
    fragility: &[f64],
    display: &[f64],
) -> Option<ModelLandingScar> {
    let faces = mesh.tris().len();
    if fragility.len() != faces {
        return None;
    }
    let landings = model_landings(plan);
    let interior = landings.iter().filter(|landing| landing.interior).count();
    if landings.is_empty() {
        return Some(ModelLandingScar {
            raw_mm2: 0.0,
            landings: 0,
            mapped: 0,
            interior: 0,
            charges: Vec::new(),
        });
    }

    // One read of the vertex triples for the whole batch, so the parallel scan
    // below touches a flat slice rather than chasing two indirections per
    // face per landing.
    let verts = mesh.verts();
    let triangles: Vec<[[f64; 3]; 3]> = mesh
        .tris()
        .iter()
        .map(|triangle| {
            [
                verts[triangle[0] as usize],
                verts[triangle[1] as usize],
                verts[triangle[2] as usize],
            ]
        })
        .collect();

    crate::perf::add(
        crate::PerfWork::NearestModelLandingRows,
        crate::perf::count(landings.len()),
    );
    crate::perf::add(
        crate::PerfWork::NearestModelLandingTrisTested,
        crate::perf::count(landings.len()).saturating_mul(crate::perf::count(triangles.len())),
    );
    // One parallel pass over the landing set. Ties go to the lower face index,
    // which is `geom_kernel`'s own nearest-face rule, so two runs of the same
    // pose charge the same face.
    let charged: Vec<Option<LandingCharge>> = landings
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
                // The same association the down-facing fold uses: the
                // fragility product first, the display increment (NOT scaled
                // by fragility) on top of it, so a zero display field
                // reproduces `contact × fragility` rather than approximating
                // it.
                let fragile = contact * fragility[face];
                LandingCharge {
                    strut: landing.strut,
                    face,
                    distance_mm: best_distance.sqrt(),
                    scar_mm2: fragile + contact * display.get(face).copied().unwrap_or(0.0),
                    interior: landing.interior,
                }
            })
        })
        .collect();

    // Summed serially in landing order, so the total is the same bits on every
    // run whatever the thread pool did above.
    let charges: Vec<LandingCharge> = charged.into_iter().flatten().collect();
    let mut raw_mm2 = 0.0_f64;
    for charge in &charges {
        raw_mm2 += charge.scar_mm2;
    }
    Some(ModelLandingScar {
        raw_mm2,
        landings: landings.len(),
        mapped: charges.len(),
        interior,
        charges,
    })
}
