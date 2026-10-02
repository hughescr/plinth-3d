//! **The Morel base's long posts** (fix/morel-long-posts, 2026-10-02).
//!
//! The owner's ChituBox findings on `for-craig-v10/Morel_base`: (1) a
//! cross-brace end that does not meet the post it braces; (2) long posts run
//! from the build plate up to the mushroom caps' undersides instead of landing
//! on the base's top face below them; (3) two of those posts pass through the
//! model. Measured on that file (`lanes/morel-long-posts.md`):
//!
//! * the four long posts (records 578, 712, 826, 1162; arms 116, 117, 120,
//!   127) had stood on the base's top face, three of them straight down, until
//!   the lip-seat rule re-landed them on the plate: its re-landing search asked
//!   only the member index, so a column whose landing was model material read
//!   as empty and was offered as the plate;
//! * two of them (826, 1162) entered the base rim by 0.208 and 0.185 mm: the
//!   rule's solid-clearance gate admitted a move "no nearer the model than it
//!   already was", and a member standing on the model read its own landing
//!   strike as standing 0.05 mm inside it;
//! * the four |X| diagonals on post 578 ended 0.08-0.26 mm off its surface:
//!   a leaning plate post's file shaft runs to its boss top OVER ITS FOOT, not
//!   along the planned seat-to-plate line the diagonal was placed on.
//!
//! The brace-end test runs on a committed fixture in the gate. The Morel test
//! is `#[ignore]`d: the Morel base is a local, uncommitted mini (the same file
//! `morel_pose.rs` reads; `PLINTH_MINIS_DIR` points elsewhere).

use crate::harness;

use plinth_core::{Finite, LayerPlan, PositiveFinite};
use plinth_mesh::{load_binary_stl, make_laid_tube, WeldPolicy};
use plinth_support::{lower_sweep_arms, BuildFrame, SupportPrimitive, SupportSegmentKind, SweepArm};
use plinth_sweep::{arms_of, Landing, StrutKind, SweepPlan, SweepPolicy};

fn policy() -> SweepPolicy {
    SweepPolicy::from_registry().expect("the registry supplies every number the sweep reads")
}

/// Whether `point` lies inside the drawn frustum `primitive` (its axis from
/// start to end, its radius linear between the two end radii), to within a
/// micrometre.
fn inside_frustum(primitive: &SupportPrimitive, point: [f64; 3]) -> bool {
    let a = primitive.start_mm.map(Finite::get);
    let b = primitive.end_mm.map(Finite::get);
    let axis: [f64; 3] = std::array::from_fn(|k| b[k] - a[k]);
    let length2: f64 = axis.iter().map(|v| v * v).sum();
    if length2 <= 0.0 {
        return false;
    }
    let rel: [f64; 3] = std::array::from_fn(|k| point[k] - a[k]);
    let t = rel.iter().zip(&axis).map(|(r, x)| r * x).sum::<f64>() / length2;
    if !(-1e-9..=1.0 + 1e-9).contains(&t) {
        return false;
    }
    let radial: f64 = (0..3)
        .map(|k| (rel[k] - t * axis[k]).powi(2))
        .sum::<f64>()
        .sqrt();
    let radius = primitive.start_radius_mm.get()
        + t.clamp(0.0, 1.0) * (primitive.end_radius_mm.get() - primitive.start_radius_mm.get());
    radial <= radius + 1e-6
}

/// The drawn solid of ONE member, lowered on its own exactly as the field's
/// lowering draws it (its shaft records and its root flare).
fn drawn_solid(plan: &SweepPlan, arm: &SweepArm) -> Vec<SupportPrimitive> {
    let layers = LayerPlan::new(plan.plate_z_mm, vec![plan.layer_height_mm; plan.layers])
        .expect("the plan's own uniform layer plan");
    let frame = BuildFrame::new(
        Finite::new(plan.plate_z_mm).expect("a finite plate"),
        PositiveFinite::new(harness::LIFT_MM).expect("a positive lift"),
    );
    lower_sweep_arms(std::slice::from_ref(arm), frame, &layers, 100_000, 1 << 30)
        .expect("the lowering refuses only about the request")
        .primitives
        .into_iter()
        .filter(|primitive| {
            matches!(
                primitive.segment_kind,
                SupportSegmentKind::ContactTaper
                    | SupportSegmentKind::Member
                    | SupportSegmentKind::RootFlare
            )
        })
        .collect()
}

/// Every brace end as the file draws it, with the post it is tied to there:
/// `(brace id, end, post id, point)`. The seat end is on `seat_on_strut`, the
/// landing end on the member it lands on.
fn brace_ends(plan: &SweepPlan, arms: &[SweepArm]) -> Vec<(u32, &'static str, u32, [f64; 3])> {
    plan.struts
        .iter()
        .zip(arms)
        .filter(|(strut, _)| {
            strut.kind == StrutKind::Brace
                && strut.k_bottom != usize::MAX
                && strut.k_top > strut.k_bottom
        })
        .flat_map(|(brace, arm)| {
            let seat = brace.seat_on_strut.map(|post| (brace.id, "seat", post, arm.seat_mm));
            let landing = match brace.landing {
                Landing::Strut { id, .. } => Some((brace.id, "landing", id, arm.base_mm)),
                Landing::Plate | Landing::Model { .. } => None,
            };
            [seat, landing].into_iter().flatten()
        })
        .collect()
}

/// **Every brace end meets its post's DRAWN solid** (rule (a)). Graded
/// against the post as the lowering draws it, alone, never against the
/// crate's own idea of where the post stands. Non-vacuous: braces are tied to
/// leaning plate posts, where the drawn shaft leaves the planned line.
#[test]
fn every_brace_end_on_the_laid_tube_meets_its_posts_drawn_solid() {
    let mesh = make_laid_tube().expect("the laid tube fixture");
    let plan = harness::sweep_with(&mesh, 0.0, &policy());
    let arms = arms_of(&plan);
    let ends = brace_ends(&plan, &arms);
    assert!(!ends.is_empty(), "anti-vacuity: the laid tube placed no brace");

    let mut solids: std::collections::BTreeMap<u32, Vec<SupportPrimitive>> =
        std::collections::BTreeMap::new();
    let mut off: Vec<String> = Vec::new();
    let mut on_leaning_plate_posts = 0;
    for &(brace, end, post, point) in &ends {
        let Some(index) = plan.struts.iter().position(|strut| strut.id == post) else {
            off.push(format!("brace {brace} {end}: post {post} is not in the field"));
            continue;
        };
        let host = &plan.struts[index];
        if host.landing == Landing::Plate && host.kind != StrutKind::Brace && host.is_leaned() {
            on_leaning_plate_posts += 1;
        }
        let solid = solids
            .entry(post)
            .or_insert_with(|| drawn_solid(&plan, &arms[index]));
        if !solid.iter().any(|primitive| inside_frustum(primitive, point)) {
            let nearest = solid
                .iter()
                .map(|primitive| {
                    let a = primitive.start_mm.map(Finite::get);
                    let b = primitive.end_mm.map(Finite::get);
                    let t = ((point[2] - a[2]) / (b[2] - a[2])).clamp(0.0, 1.0);
                    let x = a[0] + t * (b[0] - a[0]);
                    let y = a[1] + t * (b[1] - a[1]);
                    (point[0] - x).hypot(point[1] - y)
                        - (primitive.start_radius_mm.get()
                            + t * (primitive.end_radius_mm.get() - primitive.start_radius_mm.get()))
                })
                .fold(f64::INFINITY, f64::min);
            off.push(format!(
                "brace {brace} {end} at ({:.3}, {:.3}, {:.3}) stands {nearest:.4} mm outside post \
                 {post} ({:?}, leaned {})",
                point[0],
                point[1],
                point[2],
                host.landing,
                host.is_leaned()
            ));
        }
    }
    println!(
        "laid tube: {} brace ends, {on_leaning_plate_posts} of them on a leaning plate post, {} \
         off their post's drawn solid",
        ends.len(),
        off.len()
    );
    // PATH HIT: the ends this rule moves are there to move.
    assert!(
        on_leaning_plate_posts > 0,
        "anti-vacuity: no brace is tied to a leaning plate post"
    );
    assert!(
        off.is_empty(),
        "{} brace ends miss their post's drawn solid:\n{}",
        off.len(),
        off.join("\n")
    );
}

/// **On the Morel base itself, as authored** (the pose its production search
/// commits): no arm the lip-seat rule re-lands over the base is sent to the
/// plate, some are re-landed on the model, and no re-landed arm's drawn solid
/// enters the model, as the finished field's census of every member's drawn
/// solid reads it (rules (b) and (c)); those inside the final gap are printed.
#[test]
#[ignore = "reads the local, uncommitted Morel base mini (PLINTH_MINIS_DIR)"]
fn the_morel_cap_arms_land_on_the_base_and_clear_it() {
    let path = crate::morel_pose::morel_path();
    let raw = load_binary_stl(&path, WeldPolicy::None)
        .unwrap_or_else(|error| panic!("{}: {error:?}", path.display()));
    // The production chain up to placement, as `prepare.rs` runs it: repair,
    // then the registered decimation (the raw mini is too dense for the
    // model surface index the clearance rules read). As authored, which is
    // the pose the production search commits for it.
    let repaired = plinth_mesh::repair_mesh(&raw).into_mesh();
    let mesh = plinth_decimate::run(&repaired, crate::morel_pose::registered_decimation_budget())
        .expect("decimation must run")
        .into_decimated_model()
        .expect("scale sanity must pass")
        .into_parts()
        .0;
    let plan = harness::sweep_with(&mesh, 0.0, &policy());
    let lip = &plan.report.lip_seat;
    let census = &plan.report.member_model_clearance;
    // The base is a disc under the caps: an arm re-landed on the plate from a
    // seat over the disc's footprint ran down past its rim, which is what the
    // owner saw. The footprint is the mesh's own: its XY bounding box's
    // inscribed circle, less the 0.6 mm placement margin a member keeps.
    let (lo, hi) = mesh.verts().iter().fold(
        ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]),
        |(lo, hi), v| ([lo[0].min(v[0]), lo[1].min(v[1])], [hi[0].max(v[0]), hi[1].max(v[1])]),
    );
    let centre = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
    let radius = ((hi[0] - lo[0]).min(hi[1] - lo[1])) / 2.0 - 0.6;
    let grid = &plan.grid;
    let on_plate: Vec<String> = plan
        .struts
        .iter()
        .filter(|strut| lip.relanded_ids.contains(&strut.id) && strut.landing == Landing::Plate)
        .filter(|strut| {
            let x = grid.center_x_mm(grid.col_of(strut.cell));
            let y = grid.center_y_mm(grid.row_of(strut.cell));
            (x - centre[0]).hypot(y - centre[1]) < radius
        })
        .map(|strut| format!("arm {} on the plate from layer {}", strut.id, strut.k_top))
        .collect();
    // The gate: a move never puts a member's solid into the model, and never
    // nearer it than the final gap unless it already stood nearer
    // (for-craig-v10: two re-landed posts 0.208 and 0.185 mm INTO the base
    // rim). Read on the finished field by the census, not by the gate.
    let relanded_near: Vec<_> = census
        .within_gap
        .iter()
        .filter(|near| lip.relanded_ids.contains(&near.member))
        .collect();
    let relanded_inside: Vec<_> = relanded_near
        .iter()
        .filter(|near| near.clearance_mm < 0.0)
        .collect();
    println!(
        "Morel base: {} re-landed ({} on the model); census {} members over {} spans, smallest \
         gap {:?}, {} within {} mm",
        lip.arms_relanded,
        lip.relanded_on_model,
        census.members_measured,
        census.rows,
        census.smallest_clearance_mm,
        census.within_gap.len(),
        census.gap_mm,
    );
    assert!(lip.arms_relanded > 0, "anti-vacuity: the rule re-landed nothing");
    assert!(lip.relanded_on_model > 0, "PATH HIT: no arm was re-landed on the model");
    assert!(
        on_plate.is_empty(),
        "arms over the base re-landed on the plate:\n{}",
        on_plate.join("\n")
    );
    assert!(census.ran, "the census did not run");
    for near in &relanded_near {
        println!(
            "re-landed arm {} stands {:.4} mm from the model, inside the {} mm gap (it stood \
             nearer before it moved)",
            near.member, near.clearance_mm, census.gap_mm
        );
    }
    assert!(
        relanded_inside.is_empty(),
        "re-landed arms whose drawn solid enters the model: {relanded_inside:?}"
    );
}
