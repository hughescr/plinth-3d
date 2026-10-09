//! **A high-poly model keeps every rung** (fix/surface-budget, 2026-10-09).
//!
//! The owner saw long 0.05 mm-radius posts from the plate to a wing on Bat Cat
//! Owl in ChituBox ("will just not print"). Cause
//! (`field-2026-09-28/rc-2026-10-08/spindly-diagnosis.md`): on every mesh over
//! about 94k faces the sweep never built the model surface, because its index
//! estimate was over `surface_index_max_bytes`, which borrowed the 64 MiB
//! decimation placeholder. With no surface, thickening is refused "for the
//! model", the D8 model ties never run and no head is built, and the only
//! remedy left for an isolated minimum-radius post is gone. Every reference
//! mesh is under that cap, so nothing in the suite saw it.
//!
//! The fixture is a "lonely wing": a 2 mm stalk with a thin slab on top whose
//! two ends overhang the stalk by 1.2 mm, 9 mm up. Each overhang stands alone
//! (no post within the cross's reach), so thickening is the only rung that can
//! hold its post. The same solid is midpoint-subdivided past 150k faces (each
//! triangle into four; same geometry) and run at the PRODUCTION policy.
//!
//! The twin cuts the budget to one byte: the surface is not built, the run must
//! say so (`model_surface_built` false) and the wing's posts are left thin and
//! over their limit. That is the proof the test exercises the path.
//!
//! The production policy is used as it stands. The lonely peak (a 30 mm
//! stalk) is NOT added: at the ruled S = 30 its 35 mm arms need a root past the
//! 1.0 mm maximum, so it warns with or without a surface
//! (`harness::PATH_FIXTURE_SLENDERNESS_RATIO`), and it would hide this defect.

use std::collections::HashMap;

use geom_kernel::NativeSurface;
use plinth_mesh::{make_peaks_with, Mesh};
use plinth_sweep::{Landing, SweepPlan, SweepPolicy, SweepStrut, MODEL_SURFACE_MAX_BYTES};

use crate::harness;

/// Faces the subdivided fixture must exceed.
const FACES_AT_LEAST: usize = 150_000;

/// The 64 MiB the sweep used to borrow from the decimation probe: the cap the
/// fixture has to be over for this test to see the old defect at all.
const OLD_BORROWED_CAP_BYTES: usize = 64 << 20;

/// One midpoint pass: every triangle becomes four, over the same surface.
/// Midpoints are shared per undirected edge, so the mesh stays watertight.
fn subdivide_once(mesh: &Mesh) -> Mesh {
    let mut verts = mesh.verts().to_vec();
    let mut midpoints: HashMap<(u32, u32), u32> = HashMap::new();
    let mut tris = Vec::with_capacity(mesh.tris().len() * 4);
    let mut mid = |a: u32, b: u32, verts: &mut Vec<[f64; 3]>| -> u32 {
        let key = (a.min(b), a.max(b));
        *midpoints.entry(key).or_insert_with(|| {
            let (p, q) = (verts[a as usize], verts[b as usize]);
            verts.push(std::array::from_fn(|k| 0.5 * (p[k] + q[k])));
            u32::try_from(verts.len() - 1).expect("a fixture-sized vertex count")
        })
    };
    for tri in mesh.tris() {
        let [a, b, c] = *tri;
        let (ab, bc, ca) = (
            mid(a, b, &mut verts),
            mid(b, c, &mut verts),
            mid(c, a, &mut verts),
        );
        tris.extend([[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]]);
    }
    Mesh::new(verts, tris).expect("a midpoint-subdivided mesh is a mesh")
}

/// The lonely wing: a 2 x 2 x 9 mm stalk under a 4.4 x 2 x 1.5 mm slab, so each
/// end of the slab overhangs the stalk by 1.2 mm.
fn lonely_wing() -> Mesh {
    make_peaks_with([2.0, 2.0, 9.0], [4.4, 2.0, 1.5], &[0.0]).expect("the lonely wing fixture")
}

/// The wing, subdivided until it has more than [`FACES_AT_LEAST`] faces.
fn lonely_wing_high_poly() -> Mesh {
    let mut mesh = lonely_wing();
    while mesh.tris().len() <= FACES_AT_LEAST {
        mesh = subdivide_once(&mesh);
    }
    mesh
}

fn production() -> SweepPolicy {
    SweepPolicy::from_registry().expect("the registry supplies every number the sweep reads")
}

/// The posts that rise to the wing: plate-rooted arms whose top is the
/// highest in the field (the stalk's own foot post tops out far lower).
fn wing_posts(plan: &SweepPlan) -> Vec<&SweepStrut> {
    let plate_posts: Vec<&SweepStrut> = plan
        .struts
        .iter()
        .filter(|strut| matches!(strut.landing, Landing::Plate))
        .collect();
    let top = plate_posts.iter().map(|strut| strut.k_top).max().unwrap_or(0);
    plate_posts
        .into_iter()
        .filter(|strut| strut.k_top == top)
        .collect()
}

fn root_and_tip(strut: &SweepStrut) -> (f64, f64) {
    (
        *strut.radius_mm.first().expect("a post has a root"),
        *strut.radius_mm.last().expect("a post has a tip"),
    )
}

/// Anti-vacuity: the fixture is over the cap the sweep used to borrow, so on
/// the old budget it WOULD have had no surface. Without this the test could
/// pass on a mesh the old code also handled.
#[test]
fn the_fixture_is_over_the_old_borrowed_cap() {
    let mesh = lonely_wing_high_poly();
    assert!(mesh.tris().len() > FACES_AT_LEAST);
    let policy = production();
    let estimate = NativeSurface::estimate_index_memory_from_indexed(
        mesh.verts(),
        mesh.tris(),
        policy.surface_index_pitch_mm,
    )
    .expect("the estimate is defined for a closed fixture");
    assert!(
        estimate.peak_bytes() > OLD_BORROWED_CAP_BYTES,
        "the fixture's surface index ({} bytes) must be over the old 64 MiB cap",
        estimate.peak_bytes()
    );
    assert!(
        estimate.peak_bytes() as u64 <= policy.surface_index_max_bytes,
        "and under the sweep's own budget"
    );
    assert_eq!(policy.surface_index_max_bytes, MODEL_SURFACE_MAX_BYTES);
}

/// A model over the old cap gets its surface, thickens its lonely posts and
/// leaves no run over its limit.
#[test]
fn a_high_poly_model_keeps_every_rung() {
    let mesh = lonely_wing_high_poly();
    let plan = harness::sweep_with(&mesh, 0.0, &production());

    assert!(
        plan.report.model_surface_built,
        "the sweep must build the surface of a {}-face model",
        mesh.tris().len()
    );
    let lane = &plan.report.slenderness_lane;
    assert!(
        lane.warnings.is_empty(),
        "no member run may stand over its limit on the finished field: {:?}",
        lane.warnings
    );
    let posts = wing_posts(&plan);
    assert!(
        posts.len() >= 2,
        "anti-vacuity: both wing ends have a post (found {})",
        posts.len()
    );
    for post in posts {
        let (root, tip) = root_and_tip(post);
        assert!(
            root > 1.5 * tip,
            "wing post {} must be thickened at its root: root {root:.4} mm, tip {tip:.4} mm",
            post.id
        );
    }
}

/// The sabotage twin: a one-byte budget builds no surface, and the run SAYS so.
/// The same fixture then keeps its thin posts and its warnings, which proves
/// the test above is exercising the surface path and not passing on its own.
#[test]
fn without_the_surface_the_run_warns() {
    let mesh = lonely_wing_high_poly();
    let mut policy = production();
    policy.surface_index_max_bytes = 1;
    let plan = harness::sweep_with(&mesh, 0.0, &policy);

    assert!(
        !plan.report.model_surface_built,
        "a surface over the budget is not built"
    );
    assert!(
        !plan.report.slenderness_lane.warnings.is_empty(),
        "and the posts it would have thickened stand over their limit, as warnings"
    );
    for post in wing_posts(&plan) {
        let (root, tip) = root_and_tip(post);
        assert!(
            (root - tip).abs() < 1e-12,
            "no surface, no thickening: root {root:.4} mm, tip {tip:.4} mm"
        );
    }
}
