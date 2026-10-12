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

/// Whether `point` lies inside the drawn `primitive`, to within a micrometre:
/// a ball (contact or joint sphere, whose two records are its poles) or a
/// frustum (its axis from start to end, its radius linear between the two end
/// radii), and in either case on the support side of its seat trim, where the
/// file keeps it.
fn inside_drawn(primitive: &SupportPrimitive, point: [f64; 3]) -> bool {
    let kept = primitive
        .seat_trim
        .is_none_or(|trim| trim.support_side_distance_mm(point) >= -1e-6);
    let inside = match primitive.segment_kind {
        SupportSegmentKind::ContactSphere | SupportSegmentKind::JointSphere => {
            inside_ball(primitive, point)
        }
        _ => inside_frustum(primitive, point),
    };
    kept && inside
}

/// Whether `point` lies inside the ball whose poles are `primitive`'s two
/// records.
fn inside_ball(primitive: &SupportPrimitive, point: [f64; 3]) -> bool {
    let a = primitive.start_mm.map(Finite::get);
    let b = primitive.end_mm.map(Finite::get);
    let distance: f64 = (0..3)
        .map(|k| (point[k] - 0.5 * (a[k] + b[k])).powi(2))
        .sum::<f64>()
        .sqrt();
    distance <= primitive.start_radius_mm.get() + 1e-6
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
/// lowering draws it: its head ball, its shaft records, its joint balls and
/// its root flare.
///
/// RE-DERIVED 2026-10-02 (merge of develop 7826593: slenderness S 40 and the
/// creep-45 pass re-runs): the head ball is part of the post. On the merged
/// laid tube brace 157 seats on vertical plate post 1 (radius 0.927 mm, seat
/// at z 0.0) at z −0.750, 0.067 mm above the top of the post's shaft record
/// (z −0.817) and 0.067 mm from the centre of its 1.236 mm contact ball, on
/// the support side of the seat trim. Graded against the shaft and flare
/// alone it read "−0.927 mm outside": inside the post's radius but above its
/// shaft record. The brace end meets the drawn post; the grading left out
/// part of the post (`lanes/morel-long-posts/r12/diag-brace157b.log`).
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
                SupportSegmentKind::ContactSphere
                    | SupportSegmentKind::ContactTaper
                    | SupportSegmentKind::JointSphere
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

/// What [`brace_end_census`] measured on one field.
struct EndCensus {
    ends: usize,
    on_leaning_plate_posts: usize,
    /// Ends on a leaning plate post whose point ON THE PLANNED LINE, at the
    /// end's own height, would lie outside the post's drawn solid: the ends
    /// rule (a) is what puts on their post. The path hit.
    planned_line_misses: usize,
    only_in_a_ball: usize,
    off: Vec<String>,
}

/// Every brace end of `plan` graded against its post's drawn solid.
fn brace_end_census(plan: &SweepPlan) -> EndCensus {
    let arms = arms_of(plan);
    let ends = brace_ends(plan, &arms);

    let mut solids: std::collections::BTreeMap<u32, Vec<SupportPrimitive>> =
        std::collections::BTreeMap::new();
    let mut off: Vec<String> = Vec::new();
    let mut on_leaning_plate_posts = 0;
    let mut planned_line_misses = 0;
    let mut only_in_a_ball = 0;
    for &(brace, end, post, point) in &ends {
        let Some(index) = plan.struts.iter().position(|strut| strut.id == post) else {
            off.push(format!("brace {brace} {end}: post {post} is not in the field"));
            continue;
        };
        let host = &plan.struts[index];
        let leaning_plate_post =
            host.landing == Landing::Plate && host.kind != StrutKind::Brace && host.is_leaned();
        let solid = solids
            .entry(post)
            .or_insert_with(|| drawn_solid(plan, &arms[index]));
        if leaning_plate_post {
            on_leaning_plate_posts += 1;
            // The post's planned line (seat to base on the plate), at this
            // end's height.
            let (seat, base) = (arms[index].seat_mm, arms[index].base_mm);
            let rise = seat[2] - base[2];
            if rise > 0.0 {
                let t = (point[2] - base[2]) / rise;
                let planned: [f64; 3] =
                    std::array::from_fn(|k| base[k] + t * (seat[k] - base[k]));
                if !solid.iter().any(|primitive| inside_drawn(primitive, planned)) {
                    planned_line_misses += 1;
                }
            }
        }
        let in_a_ball = |primitive: &SupportPrimitive| {
            matches!(
                primitive.segment_kind,
                SupportSegmentKind::ContactSphere | SupportSegmentKind::JointSphere
            )
        };
        if solid
            .iter()
            .any(|primitive| in_a_ball(primitive) && inside_drawn(primitive, point))
            && !solid
                .iter()
                .any(|primitive| !in_a_ball(primitive) && inside_drawn(primitive, point))
        {
            only_in_a_ball += 1;
        }
        if !solid.iter().any(|primitive| inside_drawn(primitive, point)) {
            // How far outside: the radial reading at the end's own height on
            // each shaft record, the surface distance on each ball.
            let nearest = solid
                .iter()
                .map(|primitive| {
                    let a = primitive.start_mm.map(Finite::get);
                    let b = primitive.end_mm.map(Finite::get);
                    if in_a_ball(primitive) {
                        let distance: f64 = (0..3)
                            .map(|k| (point[k] - 0.5 * (a[k] + b[k])).powi(2))
                            .sum::<f64>()
                            .sqrt();
                        return distance - primitive.start_radius_mm.get();
                    }
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
    EndCensus {
        ends: ends.len(),
        on_leaning_plate_posts,
        planned_line_misses,
        only_in_a_ball,
        off,
    }
}

/// **Every brace end meets its post's DRAWN solid** (rule (a)). Graded
/// against the post as the lowering draws it, alone, never against the
/// crate's own idea of where the post stands.
///
/// Two fields: production, and the witness field, where the rule is what
/// keeps ends on their posts. RE-DERIVED 2026-10-02 (merge of develop
/// 7826593): on the merged production laid tube only 8 of 320 brace ends are
/// on a leaning plate post, and every one of them would also meet the post on
/// the planned line, so deleting the rule (sabotage S4: the adapter keeps the
/// planned line) left the production field green
/// (`lanes/morel-long-posts/r12/sabotage-S4-merged.log`). The witness field
/// is the same fixture with the creep-45 overhang cut off
/// (`without_creep_overhang_cut`, the owner's 2026-09-30 ruling), which keeps
/// the creep arms whose leaning posts the braces tie to; the path hit
/// (`planned_line_misses > 0`) is asserted there.
#[test]
fn every_brace_end_on_the_laid_tube_meets_its_posts_drawn_solid() {
    let mesh = make_laid_tube().expect("the laid tube fixture");
    for (name, policy, witness) in [
        ("production", policy(), false),
        // RE-PINNED 2026-10-03 (fix/brace-window-cap, rules R2 and R7 + D5):
        // the witness field also runs with the brace cap and the brace window
        // off. With both in force its 228 brace ends include 4 on leaning
        // plate posts and none that would miss on the planned line; with the
        // cap alone off, 236 and 4 and none; with both off, 448 ends, 60 on
        // leaning plate posts, 23 of those missing on the planned line
        // (measured 2026-10-03). The drawn-line rule reads neither rule.
        // RE-PINNED 2026-10-05 (fix/drawn-solid-clearance, challenger round 2):
        // the witness field also keeps the retired centreline descent. With the
        // drawn-solid descent (a preferred tier now also requires the final gap)
        // the same field has 408 ends, 18 on leaning plate posts and none
        // missing on the planned line; with the retired descent, 20 and 1
        // (measured 2026-10-05; the model refused 0 thickenings in either).
        // The rule under test lives in the adapter and does not read the
        // descent.
        // RE-PINNED 2026-10-11 (merge of quality Q12, Q1 + Q2, into develop):
        // the witness field also refuses the slenderness merge
        // (`without_slenderness_merge`, the remedy-1 sabotage knob). Q1 keeps
        // the seat of a lean the slenderness law cannot hold, so the same
        // field has 348 ends, 22 on leaning plate posts and none missing on the
        // planned line (was 448, 30 and 3, measured 2026-10-11 on develop
        // 2d2381a, the same fixture and knobs). With the merge refused every
        // run the merge would have joined is braced instead: 1028 ends, 48 on
        // leaning plate posts and 21 missing on the planned line (measured
        // 2026-10-11, release). The rule under test lives in the adapter and
        // reads neither the merge nor the lip-seat lean.
        // REVIEW after the Q12-regression fix (quality/q12-regressions-2026-10-11):
        // this knob masks the Q1 effect on the witness field; drop it if the
        // fix brings the planned-line misses back without it.
        (
            "creep cut off",
            policy()
                .without_creep_overhang_cut()
                .without_brace_cap()
                .without_brace_window()
                .without_drawn_solid_descent()
                .without_slenderness_merge(),
            true,
        ),
    ] {
        let plan = harness::sweep_with(&mesh, 0.0, &policy);
        let census = brace_end_census(&plan);
        println!(
            "laid tube ({name}): {} brace ends, {} of them on a leaning plate post, {} of those \
             off the post on its planned line, {} met only by a ball of their post, {} off their \
             post's drawn solid",
            census.ends,
            census.on_leaning_plate_posts,
            census.planned_line_misses,
            census.only_in_a_ball,
            census.off.len()
        );
        println!(
            "  thickenings refused by the model: {}; descents uncleared: {}",
            plan.report.slenderness_lane.thicken_refused_model, plan.report.counters.descent_solid_uncleared
        );
        assert!(census.ends > 0, "anti-vacuity ({name}): the laid tube placed no brace");
        if witness {
            // PATH HIT: some end is on its post only because it follows the
            // drawn line.
            assert!(
                census.planned_line_misses > 0,
                "anti-vacuity ({name}): no brace end on a leaning plate post would miss it on \
                 the planned line ({} on leaning plate posts)",
                census.on_leaning_plate_posts
            );
        }
        assert!(
            census.off.is_empty(),
            "{name}: {} brace ends miss their post's drawn solid:\n{}",
            census.off.len(),
            census.off.join("\n")
        );
    }
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
