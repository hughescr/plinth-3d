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
//! scar_mm2 = pi * r_tip^2 * (fragility(carried face) + D(carried face))
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

use crate::grid::Grid;
use crate::{SweepPlan, SweepStrut};

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
    /// `π r_tip² × (fragility + D)` of the carried face, computed
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
    /// `Σ π r_tip² × (fragility + D)` over charges NOT already counted by
    /// the analytic fold (the module doc's chosen half). Zero — exactly, and
    /// only — when the sweep placed no arms, or every carried face is
    /// already counted, or every carried face carries neither fragility nor
    /// display value.
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
    tip_landings_of(
        &plan.grid,
        &plan.struts,
        plan.plate_z_mm,
        plan.layer_height_mm,
    )
}

/// The same landings, over the pieces rather than the finished plan, so
/// `run_sweep` can ask for them BEFORE it has a [`SweepPlan`] to hand.
///
/// [`tip_landings`] delegates here, so there is one derivation of a tip point
/// and not two.
#[must_use]
pub fn tip_landings_of(
    grid: &Grid,
    struts: &[SweepStrut],
    plate_z_mm: f64,
    layer_height_mm: f64,
) -> Vec<TipLanding> {
    let mut landings = Vec::with_capacity(struts.len());
    for (index, strut) in struts.iter().enumerate() {
        let cell = strut.cell_at(strut.k_top);
        if cell >= grid.cells() {
            continue;
        }
        // The same slicing-plane derivation `model_landing::model_landings`
        // uses, evaluated at this arm's own `k_top` rather than its seat
        // layer.
        #[expect(
            clippy::cast_precision_loss,
            reason = "a layer index, bounded by the sweep's own layer count at ~1e4, is exact in f64"
        )]
        let z_mm = plate_z_mm + (strut.k_top as f64 + 0.5) * layer_height_mm;
        landings.push(TipLanding {
            strut: index,
            point_mm: [
                grid.center_x_mm(grid.col_of(cell)),
                grid.center_y_mm(grid.row_of(cell)),
                z_mm,
            ],
            radius_mm: strut.tip_radius_mm(),
        });
    }
    landings
}

/// Which faces a tip may CARRY (orchestrator ruling 2026-09-30).
///
/// **THE RULE: a tip seats on the face whose overhang it holds up — a face the
/// registered overhang law weighs as an overhang — and never on a wall or an
/// upward face.** A face is a seat face when it is shallower than the steep
/// end of the law's own 10-to-90 band, `centre + transition / 2` from
/// horizontal (57.5° at the registered 45° centre and 25° width). A down face
/// at `θ` from horizontal has outward normal `n_z = -cos θ`, so the test is
/// `n_z < -cos(centre + transition / 2)`.
///
/// **Why the band's steep end, and no other number.** It is the angle beyond
/// which the registered law says the model holds itself up: the weight there
/// is 0.1 and falls to 0.006 at 74° (`reach::creep_step` prices a wall's creep
/// by exactly this weight, so the up-sweep does not place a tip FOR such a
/// face). It mirrors [`crate::lean::LeanLaw`], which reads the SHALLOW end of
/// the same band; no constant is introduced, the two registered angles are the
/// only numbers read.
///
/// **What it fixes.** Until this rule the carried face was the nearest face
/// with ANY downward component (`n_z < 0`). A tip at a convex corner — beside a
/// wall, just under the edge of the face it holds — took the WALL: on the
/// recessed Round 25 fixture at the flip a leaned tip at the ring's inner
/// corner took the vertical recess wall (`n_z = -5.2e-17`, negative by the
/// orientation's rounding, not by design) and the seat trim cut its ball
/// against a vertical plane, leaving a 0.416 mm cap above the ring underside
/// (`plinth-pipeline tip_trim::round_25_at_the_flip…`). On the lane models'
/// finished plans (2026-09-30) the rule passes over a nearer wall or steep
/// face for 8 of the Morel base's 127 members (faces 1-31° off vertical, each
/// with an overhang face within 0.06 mm) and 59 of the Cylinder's 413
/// members; for none on the Round bases or the Sphere.
///
/// **Emit and warn, never refuse.** A mesh with no face the band admits keeps
/// the previous rule (every down-facing face) and the fall-back is counted
/// ([`SeatFaceHits::fallback_any_down_facing`]); a tip is never left without
/// a face because of this rule.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeatFaceRule {
    /// A face is a seat face when its outward unit normal's `z` is below this.
    max_normal_z: f64,
    /// Whether a support arm's seat normal is the area-weighted average over
    /// its contact footprint ([`crate::footprint_normal`], Craig ruling
    /// 2026-10-05) rather than its one seat triangle's. On in every
    /// constructor; the orientation prefilter and the sabotage drill turn it
    /// off ([`Self::without_footprint_normal`]).
    footprint_normal: bool,
}

impl SeatFaceRule {
    /// The rule, from the two registered angles of the overhang law.
    ///
    /// A non-finite input, or a band whose steep end is not strictly between
    /// horizontal and vertical, yields [`Self::any_down_facing`]: emit and
    /// warn, never refuse.
    #[must_use]
    pub fn from_overhang_band(centre_deg: f64, transition_deg: f64) -> Self {
        let steepest_deg = centre_deg + 0.5 * transition_deg;
        if !(steepest_deg.is_finite() && steepest_deg > 0.0 && steepest_deg < 90.0) {
            return Self::any_down_facing();
        }
        Self {
            max_normal_z: -steepest_deg.to_radians().cos(),
            footprint_normal: true,
        }
    }

    /// Every face with any downward component is a seat face: the rule before
    /// 2026-09-30. **Sabotage and comparison only.**
    #[must_use]
    pub const fn any_down_facing() -> Self {
        Self {
            max_normal_z: 0.0,
            footprint_normal: true,
        }
    }

    /// The rule with the seat normal taken from one seat triangle again, as it
    /// was before 2026-10-05. **The orientation prefilter and the sabotage
    /// drill only.**
    #[must_use]
    pub const fn without_footprint_normal(mut self) -> Self {
        self.footprint_normal = false;
        self
    }

    /// Whether a support arm's seat normal is its footprint's average
    /// ([`crate::footprint_normal::refine`]).
    #[must_use]
    pub const fn footprint_normal(&self) -> bool {
        self.footprint_normal
    }

    /// The `n_z` a face's outward unit normal must be below to be carried.
    #[must_use]
    pub const fn max_normal_z(&self) -> f64 {
        self.max_normal_z
    }

    pub(crate) fn admits(self, normal_z: f64) -> bool {
        normal_z < self.max_normal_z
    }
}

/// What the seat-face rule did on one call: the witness that it fires.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SeatFaceHits {
    /// Members given a face: one per member's top, braces included, exactly
    /// the landings [`tip_landings_of`] derives.
    pub tips: usize,
    /// Of those, members whose NEAREST down-facing face was one the rule
    /// refuses (a wall, or a face steeper than the band) and which therefore
    /// carry a different, farther face. Zero on a field where no member's top
    /// stands beside a wall; non-zero is the rule biting.
    pub steeper_face_passed_over: usize,
    /// Whether the mesh had no face the rule admits, so every down-facing face
    /// was used instead.
    pub fallback_any_down_facing: bool,
    /// Of [`Self::tips`], the model ties (Craig ruling 2026-10-03, D8), each
    /// given the nearest face of ANY slope: a tie holds a post sideways and
    /// may seat on a wall. A path-hit witness.
    pub model_ties: usize,
}

/// The OUTWARD unit normal of the face each arm's tip carries, in STRUT order
/// (not landing order), `[0, 0, 0]` where no face was found.
///
/// This is the seat law's `theta` input: the tip is driven into the model
/// along its own axis to a depth measured against this normal
/// (`plinth_support::SweepArm::seat_normal_mm`).
///
/// **Batching.** One read of the vertex triples, one parallel `landings ×
/// faces` closest-point pass, one scatter — exactly the shape
/// [`tip_contact_scar`] uses, and sharing [`nearest_faces`] with it so the
/// face a tip is CHARGED against and the face it is SEATED against can never
/// be two different answers. Nothing is queried per arm.
///
/// The mesh must be the ORIENTED mesh the plan was planned on.
#[must_use]
pub fn tip_seat_normals(
    mesh: &Mesh,
    grid: &Grid,
    struts: &[SweepStrut],
    plate_z_mm: f64,
    layer_height_mm: f64,
    rule: SeatFaceRule,
) -> Vec<[f64; 3]> {
    tip_seat_faces(mesh, grid, struts, plate_z_mm, layer_height_mm, rule)
        .into_iter()
        .map(|face| face.map_or([0.0; 3], |(normal, _)| normal))
        .collect()
}

/// One carried face, as [`tip_seat_faces`] gives it: the face's OUTWARD unit
/// normal, then its point closest to the tip's landing point.
pub type CarriedFace = ([f64; 3], [f64; 3]);

/// The face each arm's tip CARRIES, in strut order: its OUTWARD unit normal
/// and the point of it closest to the tip's landing point, or `None` where the
/// mesh has no down-facing face at all.
///
/// **The carried face is the nearest face [`SeatFaceRule`] admits** — the
/// nearest face the overhang law weighs as an overhang (2026-09-30). Before
/// that it was the nearest DOWN-FACING face (fix/tip-trim, 2026-09-28), and
/// before that the nearest face of any kind, whose normal was left unmeasured
/// when it was not down-facing. Each step removed a face the tip stands
/// BESIDE rather than under: first the upward and plain vertical faces, now
/// the walls a rotation tips a hair downward and the steep faces the law says
/// stand by themselves. Where the nearest face already was an overhang — every
/// tip under a flat or gently curved face — the answer is the same face as
/// before, bit for bit.
///
/// **Batching.** One read of the vertex triples, one filter to the admitted
/// faces, one parallel `landings × faces` closest-point pass
/// ([`nearest_faces`], shared with the scar), one scatter.
#[must_use]
pub fn tip_seat_faces(
    mesh: &Mesh,
    grid: &Grid,
    struts: &[SweepStrut],
    plate_z_mm: f64,
    layer_height_mm: f64,
    rule: SeatFaceRule,
) -> Vec<Option<CarriedFace>> {
    seat_faces(mesh, grid, struts, plate_z_mm, layer_height_mm, rule, false).0
}

/// [`tip_seat_faces`], and the MESH FACE ID of each tip's carried face (its
/// seat triangle), in strut order: the identity the footprint normal seeds its
/// connected patch from ([`crate::footprint_normal`]).
#[must_use]
pub fn tip_seat_faces_with_ids(
    mesh: &Mesh,
    grid: &Grid,
    struts: &[SweepStrut],
    plate_z_mm: f64,
    layer_height_mm: f64,
    rule: SeatFaceRule,
) -> (Vec<Option<CarriedFace>>, Vec<Option<u32>>) {
    let (faces, _, ids) = seat_faces(mesh, grid, struts, plate_z_mm, layer_height_mm, rule, false);
    (faces, ids)
}

/// [`tip_seat_faces`], and what the rule did ([`SeatFaceHits`]).
///
/// Counting needs a second closest-point pass over the faces the rule refused,
/// so it is asked for once per finished plan (`run_sweep`), not by the passes
/// that call [`tip_seat_faces`] many times while they move arms.
#[must_use]
pub fn tip_seat_faces_counted(
    mesh: &Mesh,
    grid: &Grid,
    struts: &[SweepStrut],
    plate_z_mm: f64,
    layer_height_mm: f64,
    rule: SeatFaceRule,
) -> (Vec<Option<CarriedFace>>, SeatFaceHits) {
    let (faces, hits, _) = seat_faces(mesh, grid, struts, plate_z_mm, layer_height_mm, rule, true);
    (faces, hits)
}

/// [`tip_seat_faces_counted`], and each tip's seat triangle id
/// ([`tip_seat_faces_with_ids`]).
#[must_use]
pub fn tip_seat_faces_counted_with_ids(
    mesh: &Mesh,
    grid: &Grid,
    struts: &[SweepStrut],
    plate_z_mm: f64,
    layer_height_mm: f64,
    rule: SeatFaceRule,
) -> (Vec<Option<CarriedFace>>, SeatFaceHits, Vec<Option<u32>>) {
    seat_faces(mesh, grid, struts, plate_z_mm, layer_height_mm, rule, true)
}

/// The one derivation behind the `tip_seat_faces*` family: the carried faces,
/// what the rule did, and each tip's seat triangle id (a mesh face index).
fn seat_faces(
    mesh: &Mesh,
    grid: &Grid,
    struts: &[SweepStrut],
    plate_z_mm: f64,
    layer_height_mm: f64,
    rule: SeatFaceRule,
    count: bool,
) -> (Vec<Option<CarriedFace>>, SeatFaceHits, Vec<Option<u32>>) {
    let mut faces = vec![None; struts.len()];
    let mut ids: Vec<Option<u32>> = vec![None; struts.len()];
    let mut hits = SeatFaceHits::default();
    let all_landings = tip_landings_of(grid, struts, plate_z_mm, layer_height_mm);
    if all_landings.is_empty() {
        return (faces, hits, ids);
    }
    // The mesh is outward-oriented, so a face normal already points AWAY from
    // the material — which is the direction seating moves against.
    let every_face: Vec<([[f64; 3]; 3], [f64; 3], u32)> = triangles_of(mesh)
        .into_iter()
        .enumerate()
        .filter_map(|(id, triangle)| {
            let [a, b, c] = triangle;
            let unit = vec3::normalize(vec3::cross(vec3::sub(b, a), vec3::sub(c, a)))?;
            Some((triangle, unit, u32::try_from(id).ok()?))
        })
        .collect();
    // A MODEL TIE (Craig ruling 2026-10-03, D8) holds a post sideways and
    // seats on the face it meets, a wall as readily as an overhang: it
    // carries the nearest face of ANY slope, one more batched pass over the
    // tie tips alone. Every other member keeps the rule below.
    let (tie_landings, landings): (Vec<TipLanding>, Vec<TipLanding>) = all_landings
        .into_iter()
        .partition(|landing| {
            struts
                .get(landing.strut)
                .is_some_and(|strut| strut.kind == crate::StrutKind::Tie)
        });
    if !tie_landings.is_empty() {
        let triangles: Vec<[[f64; 3]; 3]> =
            every_face.iter().map(|(triangle, _, _)| *triangle).collect();
        for (landing, found) in tie_landings.iter().zip(nearest_faces(&triangles, &tie_landings)) {
            let Some((face, _)) = found else { continue };
            let ([a, b, c], unit, id) = every_face[face];
            if let Some(slot) = faces.get_mut(landing.strut) {
                *slot = Some((unit, closest_on_tri(landing.point_mm, a, b, c)));
                ids[landing.strut] = Some(id);
                hits.tips += 1;
                hits.model_ties += 1;
            }
        }
    }
    if landings.is_empty() {
        return (faces, hits, ids);
    }
    let down_facing: Vec<([[f64; 3]; 3], [f64; 3], u32)> = every_face
        .into_iter()
        .filter(|(_, unit, _)| unit[2] < 0.0)
        .collect();
    let (admitted, refused): (Vec<_>, Vec<_>) = down_facing
        .into_iter()
        .partition(|(_, unit, _)| rule.admits(unit[2]));
    // Emit and warn: a mesh with no overhang face at all keeps every
    // down-facing face, and says so.
    let (carried, passed_over) = if admitted.is_empty() {
        hits.fallback_any_down_facing = !refused.is_empty();
        (refused, Vec::new())
    } else {
        (admitted, refused)
    };
    let triangles: Vec<[[f64; 3]; 3]> = carried.iter().map(|(triangle, _, _)| *triangle).collect();
    let nearest = nearest_faces(&triangles, &landings);
    if count && !passed_over.is_empty() {
        let steep: Vec<[[f64; 3]; 3]> =
            passed_over.iter().map(|(triangle, _, _)| *triangle).collect();
        hits.steeper_face_passed_over = nearest_faces(&steep, &landings)
            .iter()
            .zip(&nearest)
            .filter(|(steep, kept)| match (steep, kept) {
                (Some((_, steep_mm)), Some((_, kept_mm))) => steep_mm < kept_mm,
                _ => false,
            })
            .count();
    }
    for (landing, found) in landings.iter().zip(&nearest) {
        let Some((face, _)) = *found else { continue };
        let ([a, b, c], unit, id) = carried[face];
        let point = closest_on_tri(landing.point_mm, a, b, c);
        if let Some(slot) = faces.get_mut(landing.strut) {
            *slot = Some((unit, point));
            ids[landing.strut] = Some(id);
            hits.tips += 1;
        }
    }
    (faces, hits, ids)
}

/// One read of the mesh's vertex triples, in face order.
fn triangles_of(mesh: &Mesh) -> Vec<[[f64; 3]; 3]> {
    let verts = mesh.verts();
    mesh.tris()
        .iter()
        .map(|triangle| {
            [
                verts[triangle[0] as usize],
                verts[triangle[1] as usize],
                verts[triangle[2] as usize],
            ]
        })
        .collect()
}

/// The nearest face to each landing, and the distance to it, in landing order.
///
/// One parallel pass over `landings × faces`. Shared by the tip SCAR and the
/// tip SEAT so both are answered about the same face.
fn nearest_faces(
    triangles: &[[[f64; 3]; 3]],
    landings: &[TipLanding],
) -> Vec<Option<(usize, f64)>> {
    crate::perf::add(crate::PerfWork::NearestTipRows, crate::perf::count(landings.len()));
    crate::perf::add(
        crate::PerfWork::NearestTipTrisTested,
        crate::perf::count(landings.len()).saturating_mul(crate::perf::count(triangles.len())),
    );
    landings
        .par_iter()
        .map(|landing| {
            let mut best_distance = f64::INFINITY;
            let mut best_face: Option<usize> = None;
            for (face, triangle) in triangles.iter().enumerate() {
                let closest =
                    closest_on_tri(landing.point_mm, triangle[0], triangle[1], triangle[2]);
                let offset = vec3::sub(landing.point_mm, closest);
                let distance = vec3::dot(offset, offset);
                if distance < best_distance {
                    best_distance = distance;
                    best_face = Some(face);
                }
            }
            best_face.map(|face| (face, best_distance.sqrt()))
        })
        .collect()
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
    let triangles = triangles_of(mesh);
    let mut normal_z: Vec<f64> = Vec::with_capacity(faces);
    for [a, b, c] in triangles.iter().copied() {
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

    let charged: Vec<Option<TipCharge>> = nearest_faces(&triangles, &landings)
        .into_iter()
        .zip(&landings)
        .map(|(found, landing)| {
            found.map(|(face, distance_mm)| {
                let contact = core::f64::consts::PI * landing.radius_mm * landing.radius_mm;
                // Fragility and display ADD, as in `model_landing`: the
                // display increment is not scaled by fragility.
                let fragile = contact * fragility[face];
                TipCharge {
                    strut: landing.strut,
                    face,
                    distance_mm,
                    scar_mm2: fragile + contact * display.get(face).copied().unwrap_or(0.0),
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
