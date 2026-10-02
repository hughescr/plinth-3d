//! **Every member's DRAWN SOLID clears the model and every other member; the
//! crosses at one layer are credited together; a mast is a partner, never a
//! second, stiffer post** (fix/member-clearance, 2026-09-29).
//!
//! The defects, measured on develop `c33dc8e`
//! (`plinth-evidence/slice-sweep-2026-09-02/field-2026-09-28/brace-collide-diagnosis.md`):
//!
//! 1. a brace slices through the model: the stilt plate's `|X|` between posts
//!    75 and 69 enters a leg by 0.244 mm, because the path test sampled the
//!    centreline CELL only and never the brace's radius;
//! 2. the "starburst": crosses that share no post overlap each other in 8
//!    places on the stilt plate (up to 0.454 mm), because brace was tested
//!    against brace only in the same cell at the same layer;
//! 3. each cross was credited alone, so a 60° lattice took a third plane at a
//!    layer the other two already held together (20 post-layers);
//! 4. the brace pair's mast is 2.0x the arm it braces (r 0.7786 against
//!    0.3883), because the nearest cell won and its radius was grown until the
//!    panel held;
//! 5. the stilt plate's one mast (strut 198) exists only because thickening
//!    post 81 was refused by a lap its root disc already had before any growth.
//!
//! The rules (owner rulings 2026-08-22 and 2026-09-29, orchestrator ruling
//! 2026-09-29 on 5):
//!
//! * a brace's or a mast's drawn solid clears the model by the registered
//!   FINAL clearance gap (0.3 mm), end layers included;
//! * no two members that are not joined share volume;
//! * crosses meeting a post at one layer hold direction `φ` together when
//!   `Σ cos²(φ − θᵢ) ≥ ½` (½ is what one cross gives at its own 45° edge);
//! * a mast is never thicker than the member it braces, and the cell that
//!   needs the smallest mast wins;
//! * a thickening is refused for its root disc only when the growth laps a
//!   root the disc did not lap before.
//!
//! Every assertion reads the finished strut set and the fixture MESH, with the
//! test's own brute-force geometry (point-to-triangle distance and ray
//! parity); the drawn radii are the lowering's own ([`plinth_sweep::arms_of`]).
//! Nothing here restates the production clearance test.

// Test-local geometry over bounded fixture coordinates; `a·e − b·b` is the
// segment-segment determinant, not a mistyped product.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::suspicious_operation_groupings
)]

use std::collections::BTreeMap;
use std::sync::OnceLock;

use crate::harness;

use harness::sweep_with;
use plinth_core::m0_constants;
use plinth_mesh::{make_brace_pair, make_tall_stilt_plate, Mesh};
use plinth_sweep::{
    arms_of, unbraced_runs, Landing, SlendernessLaw, StrutKind, SweepPlan, SweepPolicy, SweepStrut,
};

type V = [f64; 3];

fn policy() -> SweepPolicy {
    SweepPolicy::from_registry().expect("the registry supplies every number the sweep reads")
}

fn law() -> SlendernessLaw {
    let constants = m0_constants().expect("the registry constructs");
    let placeholders = constants.placeholders();
    SlendernessLaw::new(
        placeholders.slenderness_limit_ratio().diagnostic().get(),
        placeholders.tip_radius_default_mm().diagnostic().get(),
        placeholders.tip_radius_max_legacy_mm().diagnostic().get(),
        placeholders.overhang_angle_deg().diagnostic().get()
            - 0.5
                * placeholders
                    .overhang_weight_transition_deg()
                    .diagnostic()
                    .get(),
    )
}

/// The registered final clearance gap, read here from the registry.
fn final_gap_mm() -> f64 {
    *m0_constants()
        .expect("the registry constructs")
        .evidence()
        .final_clearance_gap_mm()
        .value()
}

struct Fixture {
    mesh: Mesh,
    plan: SweepPlan,
    /// Every member's drawn radius profile, as the lowering draws it.
    drawn: Vec<Vec<f64>>,
}

fn fixture(mesh: Mesh) -> Fixture {
    fixture_with(mesh, &policy())
}

fn fixture_with(mesh: Mesh, policy: &SweepPolicy) -> Fixture {
    let plan = sweep_with(&mesh, 0.0, policy);
    let drawn = arms_of(&plan)
        .into_iter()
        .map(|arm| arm.radius_profile_mm)
        .collect();
    Fixture { mesh, plan, drawn }
}

fn stilt() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| fixture(make_tall_stilt_plate().expect("the tall stilt plate")))
}

fn brace_pair() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    // Graded at the seat placement the fixture was built for
    // (`harness::at_path_fixture_seating`): at the production placement the
    // brace pair places no brace and no mast (measured 2026-09-30), so there
    // is nothing to grade against the final gap. The clearance rules read no
    // placement.
    FIXTURE.get_or_init(|| {
        fixture_with(
            make_brace_pair().expect("the brace pair"),
            &crate::harness::at_path_fixture_seating(policy()),
        )
    })
}

fn live(strut: &SweepStrut) -> bool {
    strut.k_bottom != usize::MAX && strut.k_top > strut.k_bottom
}

fn xyz(plan: &SweepPlan, cell: usize, k: usize) -> V {
    let grid = &plan.grid;
    [
        grid.center_x_mm(grid.col_of(cell)),
        grid.center_y_mm(grid.row_of(cell)),
        plan.plate_z_mm + k as f64 * plan.layer_height_mm,
    ]
}

/// A member's axis as the lowering draws it: the seat over its seat cell at
/// `k_top`, the base over its base cell at `k_bottom` (top, bottom).
fn axis(plan: &SweepPlan, strut: &SweepStrut) -> (V, V) {
    (
        xyz(plan, strut.cell, strut.k_top),
        xyz(plan, strut.base_cell(), strut.k_bottom),
    )
}

fn widest(fixture: &Fixture, index: usize) -> f64 {
    let strut = &fixture.plan.struts[index];
    fixture.drawn[index]
        .iter()
        .copied()
        .fold(strut.min_radius_mm, f64::max)
}

fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn norm(a: V) -> f64 {
    dot(a, a).sqrt()
}
fn lerp(a: V, b: V, f: f64) -> V {
    [
        a[0] + (b[0] - a[0]) * f,
        a[1] + (b[1] - a[1]) * f,
        a[2] + (b[2] - a[2]) * f,
    ]
}

/// Point-to-triangle distance (Ericson's region test), test-local.
fn point_triangle(p: V, a: V, b: V, c: V) -> f64 {
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return norm(ap);
    }
    let bp = sub(p, b);
    let (d3, d4) = (dot(ab, bp), dot(ac, bp));
    if d3 >= 0.0 && d4 <= d3 {
        return norm(bp);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return norm(sub(ap, [ab[0] * v, ab[1] * v, ab[2] * v]));
    }
    let cp = sub(p, c);
    let (d5, d6) = (dot(ab, cp), dot(ac, cp));
    if d6 >= 0.0 && d5 <= d6 {
        return norm(cp);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return norm(sub(ap, [ac[0] * w, ac[1] * w, ac[2] * w]));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        let bc = sub(c, b);
        return norm(sub(bp, [bc[0] * w, bc[1] * w, bc[2] * w]));
    }
    let den = 1.0 / (va + vb + vc);
    let (v, w) = (vb * den, vc * den);
    norm(sub(
        ap,
        [
            ab[0] * v + ac[0] * w,
            ab[1] * v + ac[1] * w,
            ab[2] * v + ac[2] * w,
        ],
    ))
}

/// Ray parity along +Z, test-local: whether `p` is inside the closed mesh.
fn inside(mesh: &Mesh, p: V) -> bool {
    let (px, py) = (p[0] + 1.234_567e-7, p[1] + 7.654_321e-8);
    let verts = mesh.verts();
    let mut crossings = 0_usize;
    for tri in mesh.tris() {
        let (a, b, c) = (
            verts[tri[0] as usize],
            verts[tri[1] as usize],
            verts[tri[2] as usize],
        );
        let d = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
        if d == 0.0 {
            continue;
        }
        let l1 = ((b[1] - c[1]) * (px - c[0]) + (c[0] - b[0]) * (py - c[1])) / d;
        let l2 = ((c[1] - a[1]) * (px - c[0]) + (a[0] - c[0]) * (py - c[1])) / d;
        let l3 = 1.0 - l1 - l2;
        if l1 < 0.0 || l2 < 0.0 || l3 < 0.0 {
            continue;
        }
        if l1 * a[2] + l2 * b[2] + l3 * c[2] > p[2] {
            crossings += 1;
        }
    }
    crossings % 2 == 1
}

/// Signed distance to the model surface, negative inside.
fn signed(mesh: &Mesh, p: V) -> f64 {
    let verts = mesh.verts();
    let d = mesh
        .tris()
        .iter()
        .map(|t| {
            point_triangle(
                p,
                verts[t[0] as usize],
                verts[t[1] as usize],
                verts[t[2] as usize],
            )
        })
        .fold(f64::INFINITY, f64::min);
    if inside(mesh, p) {
        -d
    } else {
        d
    }
}

/// The smallest surface-to-model gap of a solid of radius `r` about `a`–`b`,
/// sampled every 0.01 mm along the axis.
fn surface_gap(mesh: &Mesh, a: V, b: V, r: f64) -> f64 {
    let samples = ((norm(sub(b, a)) / 0.01).ceil() as usize).max(2);
    (0..=samples)
        .map(|i| signed(mesh, lerp(a, b, i as f64 / samples as f64)) - r)
        .fold(f64::INFINITY, f64::min)
}

/// Segment-to-segment distance (clamped closest points), test-local.
fn segment_distance(p1: V, q1: V, p2: V, q2: V) -> f64 {
    let (d1, d2, r) = (sub(q1, p1), sub(q2, p2), sub(p1, p2));
    let (a, e, f) = (dot(d1, d1), dot(d2, d2), dot(d2, r));
    let (c, b) = (dot(d1, r), dot(d1, d2));
    let den = a * e - b * b;
    let mut s = if den > 1e-12 {
        ((b * f - c * e) / den).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mut t = (b * s + f) / e;
    if t < 0.0 {
        t = 0.0;
        s = (-c / a).clamp(0.0, 1.0);
    } else if t > 1.0 {
        t = 1.0;
        s = ((b - c) / a).clamp(0.0, 1.0);
    }
    norm(sub(lerp(p1, q1, s), lerp(p2, q2, t)))
}

/// The two posts a diagonal joins: the one it seats on and the one it lands on.
fn posts_of(strut: &SweepStrut) -> [Option<u32>; 2] {
    let base = match strut.landing {
        Landing::Strut { id, .. } => Some(id),
        Landing::Plate | Landing::Model { .. } => None,
    };
    [strut.seat_on_strut, base]
}

/// Whether two members are joined: one seats on or lands on the other, or two
/// diagonals share a post. A joint is planned contact, not interpenetration.
fn joined(plan: &SweepPlan, i: usize, j: usize) -> bool {
    let (a, b) = (&plan.struts[i], &plan.struts[j]);
    let (ia, ib) = (a.id, b.id);
    let pa = posts_of(a);
    let pb = posts_of(b);
    pa.contains(&Some(ib))
        || pb.contains(&Some(ia))
        || (a.kind == StrutKind::Brace
            && b.kind == StrutKind::Brace
            && pa.iter().any(|p| p.is_some() && pb.contains(p)))
}

// ------------------------------------------------------------------ rule 1

fn assert_braces_and_masts_clear_the_model(name: &str, fixture: &Fixture) {
    let gap = final_gap_mm();
    let plan = &fixture.plan;
    let mut graded = 0_usize;
    let mut close = Vec::new();
    for (index, strut) in plan.struts.iter().enumerate() {
        if !live(strut) || !matches!(strut.kind, StrutKind::Brace | StrutKind::Mast) {
            continue;
        }
        graded += 1;
        let (top, bottom) = axis(plan, strut);
        let found = surface_gap(&fixture.mesh, top, bottom, widest(fixture, index));
        if found < gap {
            close.push((index, strut.kind, (found * 1000.0).round() / 1000.0));
        }
    }
    println!(
        "{name}: {graded} braces and masts graded against the {gap} mm final gap; {} closer: {close:?}",
        close.len()
    );
    assert!(
        graded > 0,
        "{name}: anti-vacuity, no brace or mast to grade"
    );
    assert!(
        close.is_empty(),
        "{name}: {} braces or masts come within the {gap} mm final gap of the model \
         (index, kind, surface gap mm): {close:?}",
        close.len()
    );
}

/// Rule 1 on the stilt plate (red on `c33dc8e`: struts 180 and 181 enter the
/// leg by 0.244 mm).
#[test]
fn no_brace_or_mast_comes_within_the_final_gap_of_the_model_on_the_stilt_plate() {
    assert_braces_and_masts_clear_the_model("tall stilt plate", stilt());
}

/// Rule 1 on the brace pair (red on `c33dc8e`: X 22/23 clears its leg by only
/// 0.243 mm).
#[test]
fn no_brace_or_mast_comes_within_the_final_gap_of_the_model_on_the_brace_pair() {
    assert_braces_and_masts_clear_the_model("brace pair", brace_pair());
}

// ------------------------------------------------------------------ rule 2

/// Rule 2: no brace or mast shares volume with any member it is not joined to
/// (red on `c33dc8e`: 8 brace pairs on the stilt plate, up to 0.454 mm).
#[test]
fn no_brace_or_mast_interpenetrates_a_member_it_is_not_joined_to_on_the_stilt_plate() {
    let fixture = stilt();
    let plan = &fixture.plan;
    let members: Vec<usize> = plan
        .struts
        .iter()
        .enumerate()
        .filter(|(_, s)| live(s))
        .map(|(i, _)| i)
        .collect();
    let mut graded = 0_usize;
    let mut bad = Vec::new();
    for (x, &i) in members.iter().enumerate() {
        for &j in &members[x + 1..] {
            let (si, sj) = (&plan.struts[i], &plan.struts[j]);
            let tie = |s: &SweepStrut| matches!(s.kind, StrutKind::Brace | StrutKind::Mast);
            if !(tie(si) || tie(sj)) || joined(plan, i, j) {
                continue;
            }
            graded += 1;
            let (ti, bi) = axis(plan, si);
            let (tj, bj) = axis(plan, sj);
            let gap = segment_distance(ti, bi, tj, bj) - widest(fixture, i) - widest(fixture, j);
            if gap < -1.0e-9 {
                bad.push((i, j, (gap * 1000.0).round() / 1000.0));
            }
        }
    }
    println!(
        "tall stilt plate: {graded} member pairs graded, {} interpenetrate: {bad:?}",
        bad.len()
    );
    assert!(graded > 0, "anti-vacuity");
    assert!(
        bad.is_empty(),
        "{} member pairs interpenetrate (i, j, gap mm): {bad:?}",
        bad.len()
    );
}

// ------------------------------------------------------------------ rule 3

fn plane_deg(plan: &SweepPlan, a: u32, b: u32, k: usize) -> f64 {
    let pa = xyz(plan, plan.struts[a as usize].cell_at(k), k);
    let pb = xyz(plan, plan.struts[b as usize].cell_at(k), k);
    (pb[1] - pa[1])
        .atan2(pb[0] - pa[0])
        .to_degrees()
        .rem_euclid(180.0)
}

fn apart(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(180.0);
    d.min(180.0 - d)
}

/// The least, over every direction on the half-turn (0.1° steps), of
/// `Σ cos²(φ − θᵢ)`: the planes' joint restraint in units of one cross along
/// its own plane.
fn joint_min(planes: &[f64]) -> f64 {
    (0..1800)
        .map(|i| {
            let phi = f64::from(i) * 0.1;
            planes
                .iter()
                .map(|t| (phi - t).to_radians().cos().powi(2))
                .sum::<f64>()
        })
        .fold(f64::INFINITY, f64::min)
}

/// The distinct planes of the crosses on each (post, band-top layer).
fn planes_by_post_layer(plan: &SweepPlan) -> BTreeMap<(u32, usize), Vec<f64>> {
    let mut by: BTreeMap<(u32, usize), Vec<f64>> = BTreeMap::new();
    for strut in &plan.struts {
        if strut.kind != StrutKind::Brace || !live(strut) {
            continue;
        }
        let [Some(a), Some(b)] = posts_of(strut) else {
            continue;
        };
        let deg = plane_deg(plan, a, b, strut.k_top);
        for post in [a, b] {
            let list = by.entry((post, strut.k_top)).or_default();
            if !list.iter().any(|d| apart(*d, deg) < 1e-6) {
                list.push(deg);
            }
        }
    }
    by
}

/// Rule 3: at one layer the crosses on a post are credited TOGETHER, so no
/// cross adds a plane at a layer where, at BOTH posts it ties, the other
/// planes there already hold every direction jointly (the report's form, per
/// post, was red on `c33dc8e`: 20 post-layers on the stilt plate, e.g. post 35
/// at k 350 with planes 0, 60 and 120). A plane one post could spare but its
/// partner has as its only plane there is the partner's tie, not a surplus.
#[test]
fn no_post_takes_a_plane_its_other_planes_at_that_layer_already_hold_on_the_stilt_plate() {
    let plan = &stilt().plan;
    let by = planes_by_post_layer(plan);
    // Whether `plane` is jointly redundant among the planes at (post, k): the
    // others there still give Σ cos² ≥ ½ in every direction.
    let redundant_at = |post: u32, k: usize, plane: f64| {
        by.get(&(post, k)).is_some_and(|planes| {
            let rest: Vec<f64> = planes
                .iter()
                .copied()
                .filter(|d| apart(*d, plane) > 1e-6)
                .collect();
            rest.len() + 1 == planes.len() && joint_min(&rest) >= 0.5 - 1e-9
        })
    };
    // Every cross, as (its two posts, its band top, its plane).
    let mut crosses: Vec<(u32, u32, usize, f64)> = Vec::new();
    for strut in &plan.struts {
        if strut.kind != StrutKind::Brace || !live(strut) {
            continue;
        }
        if let [Some(a), Some(b)] = posts_of(strut) {
            let entry = (
                a.min(b),
                a.max(b),
                strut.k_top,
                plane_deg(plan, a, b, strut.k_top),
            );
            if !crosses.contains(&entry) {
                crosses.push(entry);
            }
        }
    }
    // A cross is a surplus plane when BOTH posts it ties already hold every
    // direction at that layer without it: a third plane where two hold. A
    // plane one post can spare but its partner needs (the partner's only
    // plane there) is that partner's tie, not a surplus.
    let mut surplus = Vec::new();
    for &(a, b, k, plane) in &crosses {
        if redundant_at(a, k, plane) && redundant_at(b, k, plane) {
            surplus.push((a, b, k, plane));
        }
    }
    println!(
        "tall stilt plate: {} post-layers with crosses, {} carry a plane the others hold: {:?}",
        by.len(),
        surplus.len(),
        surplus.iter().take(10).collect::<Vec<_>>()
    );
    assert!(!by.is_empty(), "anti-vacuity: no cross on the stilt plate");
    assert!(
        surplus.is_empty(),
        "{} post-layers carry a plane two others already hold jointly",
        surplus.len()
    );
}

// ------------------------------------------------------------------ rule 4

/// Every mast and the members its crosses tie, with both drawn radii.
fn masts_and_what_they_brace(fixture: &Fixture) -> Vec<(usize, f64, u32, f64)> {
    let plan = &fixture.plan;
    let mut out = Vec::new();
    for (m, mast) in plan.struts.iter().enumerate() {
        if mast.kind != StrutKind::Mast || !live(mast) {
            continue;
        }
        let rm = widest(fixture, m);
        let mut braced: Vec<u32> = plan
            .struts
            .iter()
            .filter(|b| b.kind == StrutKind::Brace && live(b))
            .filter_map(|b| {
                let [seat, base] = posts_of(b);
                let m = u32::try_from(m).ok();
                if seat == m {
                    base
                } else if base == m {
                    seat
                } else {
                    None
                }
            })
            .collect();
        braced.sort_unstable();
        braced.dedup();
        for arm in braced {
            out.push((m, rm, arm, widest(fixture, arm as usize)));
        }
    }
    out
}

/// Rule 4: a mast is never thicker than the member it braces (red on
/// `c33dc8e`: mast 30, r 0.7786, braces arms 8 and 9 at 0.3883 and 0.4402).
#[test]
fn no_mast_is_thicker_than_the_member_it_braces_on_the_brace_pair() {
    let pairs = masts_and_what_they_brace(brace_pair());
    let bad: Vec<_> = pairs
        .iter()
        .filter(|(_, rm, _, ra)| rm > &(ra + 1.0e-12))
        .collect();
    println!("brace pair: masts and the members they brace (mast, r, member, r): {pairs:?}");
    assert!(
        bad.is_empty(),
        "masts thicker than the member they brace (mast, r, member, r): {bad:?}"
    );
}

/// Rule 4 where masts are still placed: the lonely peak with the thicken rung
/// switched off (the ladder then has only crosses and masts, and its stalk
/// blocks every cross). Every mast is no thicker than what it braces, and the
/// cap FIRES (anti-vacuity: masts placed, and cells refused for needing a
/// thicker mast).
#[test]
fn no_mast_is_thicker_than_the_member_it_braces_on_the_lonely_peak_without_thickening() {
    let mesh = plinth_mesh::make_lonely_peak().expect("the lonely peak");
    // At the S this fixture was built at (`harness::PATH_FIXTURE_SLENDERNESS_RATIO`):
    // at the ruled 30 the panel check refuses every mast cell (0 masts,
    // measured 2026-10-01) and the cap graded here is never reached.
    // RE-PINNED 2026-10-02 (fix/morel-long-posts): the lip-seat rule off, as
    // in `mast_root_clearance`'s lonely-peak test and for the same reason:
    // its clearance gate now refuses one of the fixture's lip-seat
    // re-landings, and with that arm unmoved this configuration places no
    // mast, so the cap graded here was never reached.
    let plan = sweep_with(
        &mesh,
        0.0,
        &SweepPolicy {
            slenderness_thicken: false,
            lip_seat: false,
            ..crate::harness::at_path_fixture_slenderness(policy())
        },
    );
    let drawn = arms_of(&plan)
        .into_iter()
        .map(|arm| arm.radius_profile_mm)
        .collect();
    let lane = plan.report.slenderness_lane.clone();
    let fixture = Fixture { mesh, plan, drawn };
    let pairs = masts_and_what_they_brace(&fixture);
    let bad: Vec<_> = pairs
        .iter()
        .filter(|(_, rm, _, ra)| rm > &(ra + 1.0e-12))
        .collect();
    println!(
        "lonely peak, thickening off: masts and what they brace {pairs:?}; cells refused for \
         a thicker mast {}; masts moved to a thinner cell {}; warnings {}",
        lane.mast_cells_refused_thicker_than_member,
        lane.masts_moved_to_a_thinner_cell,
        lane.warnings.len()
    );
    assert!(!pairs.is_empty(), "anti-vacuity: no mast placed");
    assert!(
        lane.mast_cells_refused_thicker_than_member > 0,
        "anti-vacuity: the cap never fired"
    );
    assert!(
        bad.is_empty(),
        "masts thicker than the member they brace (mast, r, member, r): {bad:?}"
    );
}

// ------------------------------------------------------------------ rule 5

/// Rule 5 (orchestrator ruling 2026-09-29): a thickening is refused for its
/// disc only when the GROWTH laps a root the disc did not already lap. Post
/// 81's root laps its neighbour by 0.051 mm before any growth, so the rung
/// may thicken it and no mast is needed (red on `c33dc8e`: mast 198).
#[test]
fn the_stilt_plate_places_no_mast() {
    let plan = &stilt().plan;
    let lane = &plan.report.slenderness_lane;
    let masts: Vec<usize> = plan
        .struts
        .iter()
        .enumerate()
        .filter(|(_, s)| s.kind == StrutKind::Mast && live(s))
        .map(|(i, _)| i)
        .collect();
    println!(
        "tall stilt plate: masts {masts:?}; thickened {}, refused by disc {}, grown over an existing lap {}",
        lane.members_thickened, lane.thicken_refused_disc, lane.thicken_over_existing_lap
    );
    assert!(masts.is_empty(), "the stilt plate places masts {masts:?}");
}

// ------------------------------------------------------------------ rule 6

/// Rule 6 (kept): one tie at mid-height is what the cantilever rule gives the
/// long stilt posts, and it holds while PRINTING — with every cross credited
/// only from its band TOP, the first layer at which it ties anything, no run
/// is over its limit. A guard, green on `c33dc8e`.
///
/// **RE-DERIVED 2026-10-01 (Craig's rulings: tie timing; `S` = 30).** The
/// lane now measures exactly this print-time view itself: a diagonal holds
/// its post only from its band top (it is a landing that holds from its
/// seat). At `S` = 30 three stilt posts (11, 70, 76) have no admissible
/// remedy in one direction and are WARNED; every run this guard finds over
/// the limit must be one of those published warnings, never a silent one
/// (until the timing rule, runs on 21 and 43 were over here and silent:
/// measured 2026-10-01).
#[test]
fn every_run_holds_with_crosses_credited_from_their_band_top_on_the_stilt_plate() {
    let plan = &stilt().plan;
    let mut struts = plan.struts.clone();
    for strut in &mut struts {
        if strut.kind != StrutKind::Brace || !live(strut) {
            continue;
        }
        if let Landing::Strut { id, .. } = strut.landing {
            strut.landing = Landing::Strut {
                id,
                layer: strut.k_top,
            };
        }
    }
    let over: Vec<_> = unbraced_runs(&plan.grid, &struts, plan.layer_height_mm, &law())
        .into_iter()
        .filter(|run| run.over_limit())
        .map(|run| (run.strut_id, run.k_lo, run.k_top))
        .collect();
    let warned = &plan.report.slenderness_lane.warnings;
    let silent: Vec<_> = over
        .iter()
        .filter(|(id, k_lo, k_top)| {
            !warned.iter().any(|warning| {
                warning.strut_id == *id && warning.k_lo <= *k_lo && *k_top <= warning.k_top
            })
        })
        .collect();
    println!(
        "tall stilt plate: {} runs over the limit at print time {over:?}, {} not warned",
        over.len(),
        silent.len()
    );
    assert!(
        silent.is_empty(),
        "{} runs over the limit at print time and not warned: {silent:?}",
        silent.len()
    );
}
