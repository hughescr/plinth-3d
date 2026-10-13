//! **THE FOOT OF AN ARM THAT LANDS ON THE MODEL** (lane 2, 2026-10-12,
//! `plinth-evidence/.../quality-2026-10-10/joint/MODEL-FEET.md`).
//!
//! # What was wrong
//!
//! Until 2026-10-12 an arm that landed on the model ended in its shaft's own
//! flat end: no ball, no seat, no trim, no contact size. Its width was the
//! shaft's WIDEST section (the area law plus the slenderness minimum), so a
//! post sized to stand 10 mm unbraced put a 0.70-0.87 mm flat end on a wing
//! feather carrying one 0.05 mm tip's load (49-75 times the area that load
//! needs), and the claw case (Deinonychus rec 810) put a 0.566 mm end on a
//! claw narrower than the post, 44 % of it in the air. Nothing tested whether
//! the foot fit, and the scar was priced as a full disc of the widest section.
//!
//! # The rule
//!
//! * **F1, the foot is a contact.** The shaft narrows in a cone to a ball
//!   seated into the landed face by the top tip's own seat law and cut to the
//!   model mesh by the same trim (`plinth_support::SweepArmLanding::Model`).
//!   Its contact is sized by the area law for the load the foot passes,
//!   `π r² ≥ carried / X_ALLOWED`, read from the down-sweep's LOAD ledger at
//!   the landing layer, before any slenderness or head minimum widened the
//!   section there ([`crate::down_sweep::size_arm`], the same law and clamps
//!   every section is sized by). The slenderness width stays in the shaft's
//!   span; only the cone at the foot narrows from it.
//! * **The landed face** is the nearest UP-facing face of the oriented mesh to
//!   where the axis leaves the strike lamina: one batched closest-face pass
//!   ([`crate::tip_landing::nearest_faces`], the one the tips use).
//! * **The real contact** is the model surface inside the drawn foot ball,
//!   over the up-facing faces connected to the landed face through edges
//!   inside the ball, joined by position
//!   ([`crate::footprint_normal::footprint_normals`], the one the tips'
//!   real contact uses). It prices the scar ([`crate::model_landing`]).
//! * **F2, the foot must fit.** The census here measures, on the column
//!   history's cured runs, how much of each foot's load-sized contact disc
//!   stands on cured model ([`crate::up_sweep::foot_on_model`], the predicate
//!   the descent and the re-landings read). A foot not wholly on the model is
//!   WARNED with its percent (emit and warn, never refuse), and named beside
//!   the landing check, which no longer calls it rooted without saying so.
//!
//! # Cost
//!
//! One closest-face pass over the model landings, one disc census (a few
//! raster cells per foot), one batched [`plinth_support::model_foot_discs`]
//! call, one pairs query and one parallel footprint pass. Nothing per foot
//! crosses a native boundary.

use geom_kernel::{closest_on_tri, vec3};

use crate::down_sweep::{size_arm, SizingPolicy};
use crate::footprint_normal::{footprint_normals, footprint_pairs, FootprintBall, MeasureEnv};
use crate::tip_landing::{nearest_faces, triangles_of, TipLanding};
use crate::up_sweep::{foot_on_model, ColumnHistory};
use crate::{Landing, SweepPlan};

/// The face a model foot stands on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LandedFace {
    /// The face's OUTWARD unit normal (`z > 0`).
    pub normal: [f64; 3],
    /// Its point closest to where the axis leaves the strike lamina.
    pub point_mm: [f64; 3],
    /// The mesh face id: the seed of the real-contact patch.
    pub face: u32,
}

/// One model-landed arm's foot, as measured on the finished field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FootFace {
    /// The landed face; `None` where the mesh has no up-facing face at all.
    pub face: Option<LandedFace>,
    /// The load-sized contact radius: `size_arm(carried at the landing)`.
    pub contact_radius_mm: f64,
    /// What the load the foot passes needs, `carried / X_ALLOWED`, mm².
    pub need_mm2: f64,
    /// Raster cells of the contact disc on cured model, and in the disc.
    pub on_model_cells: u32,
    pub disc_cells: u32,
    /// The ball's planned disc in the landed face's plane, mm² (0 until the
    /// contact is measured).
    pub planned_mm2: f64,
    /// The model surface inside the drawn ball, mm²; `None` until measured.
    pub real_mm2: Option<f64>,
}

impl FootFace {
    /// The share of the contact disc on cured model, 0..=1.
    #[must_use]
    pub fn on_model(&self) -> f64 {
        if self.disc_cells == 0 {
            return 0.0;
        }
        f64::from(self.on_model_cells) / f64::from(self.disc_cells)
    }

    /// Whether the whole contact disc stands on the model.
    #[must_use]
    pub fn fits(&self) -> bool {
        self.on_model_cells == self.disc_cells
    }

    /// **The scar this foot leaves**, mm²: the REAL contact where it was
    /// measured and found, else the planned disc, else the load-sized disc.
    /// Never the shaft's widest section.
    #[must_use]
    pub fn scar_mm2(&self) -> f64 {
        match self.real_mm2 {
            Some(real) if real > 0.0 => real,
            _ if self.planned_mm2 > 0.0 => self.planned_mm2,
            _ => core::f64::consts::PI * self.contact_radius_mm * self.contact_radius_mm,
        }
    }
}

/// A foot not wholly on the model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FootPartial {
    /// The strut id.
    pub id: u32,
    /// Percent of the contact disc on cured model.
    pub on_model_pct: f64,
    /// The foot's contact radius, mm.
    pub contact_radius_mm: f64,
}

/// A foot whose real contact is short of what its load needs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FootShort {
    pub id: u32,
    pub real_mm2: f64,
    pub need_mm2: f64,
}

/// What the foot measurement found on one field.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelFeetOutcome {
    /// Whether it ran.
    pub enabled: bool,
    /// Model landings given a foot.
    pub feet: usize,
    /// Of those, feet with a measured landed face.
    pub faces_measured: usize,
    /// Of those, feet whose real contact was measured.
    pub contacts_measured: usize,
    /// Sums over the feet, mm²: the load's need, the planned discs, the real
    /// contact (the scar), and what the old flat ends would have been
    /// (`π r_root²`, the widest section).
    pub need_mm2: f64,
    pub planned_mm2: f64,
    pub real_mm2: f64,
    pub flat_end_mm2: f64,
    /// Feet not wholly on the model, with the percent that is.
    pub partial: Vec<FootPartial>,
    /// Feet whose real contact is more than one pixel of area short of what
    /// their load needs.
    pub short_of_need: Vec<FootShort>,
    /// The descent's F2 tallies ([`crate::up_sweep::DescentTally`]): lean
    /// candidates refused for their foot, straight descents replaced, and
    /// straight descents kept with a foot that does not fit.
    pub descent_candidates_refused: u64,
    pub descent_straight_replaced: u64,
    pub descent_straight_kept: u64,
    /// Lip-seat and clearance re-landings not offered for their foot.
    pub relanding_candidates_refused: usize,
}

/// **Measure every model-landed arm's foot on the finished field** (module
/// doc) and write it into the strut's [`crate::SeatFace::foot`], so the
/// adapter draws it ([`crate::adapter::sweep_arm`]) and the scar prices it.
#[must_use]
pub fn measure(
    env: &MeasureEnv<'_>,
    plan: &mut SweepPlan,
    history: &ColumnHistory,
    sizing: SizingPolicy,
) -> ModelFeetOutcome {
    let mut outcome = ModelFeetOutcome {
        enabled: true,
        ..ModelFeetOutcome::default()
    };
    for strut in &mut plan.struts {
        strut.seat_face.foot = None;
    }
    let feet: Vec<usize> = plan
        .struts
        .iter()
        .enumerate()
        .filter(|(_, strut)| {
            matches!(strut.landing, Landing::Model { .. }) && strut.k_bottom != usize::MAX
        })
        .map(|(index, _)| index)
        .collect();
    if feet.is_empty() {
        return outcome;
    }
    // The arms exactly as the lowering gets them (follower moves applied):
    // one per strut, in strut order.
    let arms = crate::adapter::arms_of(plan);
    let height = plan.layer_height_mm;
    let landings: Vec<TipLanding> = feet
        .iter()
        .map(|&index| {
            let base = arms[index].base_mm;
            TipLanding {
                strut: index,
                // The top of the strike lamina over the base: the raster's
                // surface the arm lands on.
                point_mm: [base[0], base[1], base[2] + height],
                radius_mm: 0.0,
            }
        })
        .collect();
    // ONE closest-face pass over the up-facing faces.
    let triangles = triangles_of(env.mesh);
    let up_facing: Vec<(u32, [f64; 3])> = triangles
        .iter()
        .enumerate()
        .filter_map(|(id, &[a, b, c])| {
            let normal = vec3::normalize(vec3::cross(vec3::sub(b, a), vec3::sub(c, a)))?;
            (normal[2] > 0.0).then_some((u32::try_from(id).ok()?, normal))
        })
        .collect();
    let candidates: Vec<[[f64; 3]; 3]> = up_facing
        .iter()
        .map(|&(id, _)| triangles[id as usize])
        .collect();
    let nearest = nearest_faces(&candidates, &landings);
    for (landing, found) in landings.iter().zip(&nearest) {
        let index = landing.strut;
        let face = found.map(|(slot, _)| {
            let (id, normal) = up_facing[slot];
            let [a, b, c] = triangles[id as usize];
            LandedFace {
                normal,
                point_mm: closest_on_tri(landing.point_mm, a, b, c),
                face: id,
            }
        });
        let strut = &plan.struts[index];
        let carried = strut.carried_mm2.first().copied().unwrap_or(0.0).max(0.0);
        let radius = size_arm(carried, sizing);
        let (on, total) =
            foot_on_model(&plan.grid, history, strut.base_cell(), strut.k_bottom, radius);
        let flat_root = strut.radius_mm.first().copied().unwrap_or(radius);
        outcome.flat_end_mm2 += core::f64::consts::PI * flat_root * flat_root;
        plan.struts[index].seat_face.foot = Some(FootFace {
            face,
            contact_radius_mm: radius,
            need_mm2: if sizing.x_allowed > 0.0 {
                carried / sizing.x_allowed
            } else {
                0.0
            },
            on_model_cells: u32::try_from(on).unwrap_or(u32::MAX),
            disc_cells: u32::try_from(total).unwrap_or(u32::MAX),
            planned_mm2: 0.0,
            real_mm2: None,
        });
    }
    // THE REAL CONTACT, on the balls the lowering will draw: one batched disc
    // call, one pairs query, one parallel footprint pass.
    let drawn = crate::adapter::arms_of(plan);
    let discs = plinth_support::model_foot_discs(&drawn, env.layer_plan).unwrap_or_default();
    let mut rows: Vec<usize> = Vec::with_capacity(feet.len());
    let mut centres: Vec<[f64; 3]> = Vec::with_capacity(feet.len());
    let mut radii: Vec<f64> = Vec::with_capacity(feet.len());
    let mut balls: Vec<FootprintBall> = Vec::with_capacity(feet.len());
    for &index in &feet {
        let (Some(Some(disc)), Some(foot)) = (discs.get(index), plan.struts[index].seat_face.foot)
        else {
            continue;
        };
        if let Some(slot) = plan.struts[index].seat_face.foot.as_mut() {
            slot.planned_mm2 = core::f64::consts::PI * disc.radius_mm * disc.radius_mm;
        }
        let Some(face) = foot.face else { continue };
        rows.push(index);
        centres.push(disc.ball_centre_mm);
        radii.push(disc.ball_radius_mm);
        balls.push(FootprintBall {
            centre_mm: disc.ball_centre_mm,
            radius_mm: disc.ball_radius_mm,
            seat_face: face.face,
            facet_normal: face.normal,
            max_normal_z: f64::INFINITY,
            min_normal_z: 0.0,
        });
    }
    if let Some(pairs) = footprint_pairs(env.mesh, env.surface, &centres, &radii) {
        let found = footprint_normals(env.mesh, &pairs, &balls);
        for (&index, answer) in rows.iter().zip(&found) {
            if let Some(slot) = plan.struts[index].seat_face.foot.as_mut() {
                slot.real_mm2 = Some(answer.map_or(0.0, |found| found.contact_area_mm2));
            }
        }
    }
    let pixel_area_mm2 = plan.grid.pitch_mm() * plan.grid.pitch_mm();
    for &index in &feet {
        let strut = &plan.struts[index];
        let Some(foot) = strut.seat_face.foot else { continue };
        outcome.feet += 1;
        outcome.faces_measured += usize::from(foot.face.is_some());
        outcome.need_mm2 += foot.need_mm2;
        outcome.planned_mm2 += foot.planned_mm2;
        outcome.real_mm2 += foot.scar_mm2();
        if let Some(real) = foot.real_mm2 {
            outcome.contacts_measured += 1;
            if real + pixel_area_mm2 < foot.need_mm2 {
                outcome.short_of_need.push(FootShort {
                    id: strut.id,
                    real_mm2: real,
                    need_mm2: foot.need_mm2,
                });
            }
        }
        if !foot.fits() {
            outcome.partial.push(FootPartial {
                id: strut.id,
                on_model_pct: 100.0 * foot.on_model(),
                contact_radius_mm: foot.contact_radius_mm,
            });
        }
    }
    outcome
}
