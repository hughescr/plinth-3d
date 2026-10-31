//! Broken files (T1n; design dev-loop.md section 6, step11-construction.md section 8.1).
//!
//! Each file here is built by test code through the production writer (`plinth-chitubox`),
//! with support parts made by the production shape builders (`plinth_emit::shapes`: the native
//! ball, frustum and raft shapes), and then handed to the real `plinth-check` program through
//! the production hand-off (`plinth_cli::check::run`). Nothing is committed as a binary.
//!
//! The frame: a closed 10 mm cube, centred, its underside at local z = -5, written with
//! translate Z equal to the lift. The build plate is then at local z = -5 - lift, and every
//! support part is placed from that. A raft pad far from everything marks the plate.
//!
//! What a result may be: a broken file for rule Hn must say what its [`Expect`] names for Hn
//! (a violation, mostly), must never let H8 pass, and must be sound apart from its rule (no
//! structure notes). A control file must come out `pass` (or `not_applicable`) for Hn.
//!
//! What this does not show: each case proves the check fires on that defect. It does not prove
//! the check complete. The smallest defect each check sees is listed per rule below ("Smallest
//! seen") and is the smallest case here, not a bound.

use plinth_chitubox::preview::BACKGROUND;
use plinth_chitubox::{Model, Preset, Preview, Project, SupportRecord, Supports, Triangle};
use plinth_emit::shapes::{self, FrustumRole, Spec, SphereRole};

const RULES: [&str; 9] = ["H1", "H2", "H3", "H4", "H5", "H6", "H7", "H8", "H9"];

type V3 = [f32; 3];

// ---- geometry the cases are made of ---------------------------------------------------

/// A closed box centred on `c` with half-width `h`, outward winding; `inward` flips it (a
/// cavity wall).
pub(crate) fn cube(c: V3, h: f32, inward: bool) -> Vec<Triangle> {
    let p = |i: usize| {
        [
            c[0] + if i & 1 != 0 { h } else { -h },
            c[1] + if i & 2 != 0 { h } else { -h },
            c[2] + if i & 4 != 0 { h } else { -h },
        ]
    };
    let quads = [
        [0, 2, 3, 1],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 4, 6, 2],
        [1, 3, 7, 5],
    ];
    let mut out = Vec::new();
    for q in quads {
        let (a, b, c2, d) = (p(q[0]), p(q[1]), p(q[2]), p(q[3]));
        if inward {
            out.push([a, c2, b]);
            out.push([a, d, c2]);
        } else {
            out.push([a, b, c2]);
            out.push([a, c2, d]);
        }
    }
    out
}

/// A square tube along z: outer half-width `a`, a square bore of half-width `b` through it,
/// height `h`, centred on the origin. Closed (both ends are frames), outward winding.
fn tube(a: f32, b: f32, h: f32) -> Vec<Triangle> {
    let corner = |half: f32, i: usize, z: f32| {
        let (sx, sy) = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)][i % 4];
        [sx * half, sy * half, z]
    };
    let (lo, hi) = (-0.5 * h, 0.5 * h);
    let mut out = Vec::new();
    for i in 0..4 {
        for (half, flip) in [(a, false), (b, true)] {
            let (b0, b1) = (corner(half, i, lo), corner(half, i + 1, lo));
            let (t0, t1) = (corner(half, i, hi), corner(half, i + 1, hi));
            if flip {
                out.push([b0, t1, b1]);
                out.push([b0, t0, t1]);
            } else {
                out.push([b0, b1, t1]);
                out.push([b0, t1, t0]);
            }
        }
        for (z, up) in [(hi, true), (lo, false)] {
            let (o0, o1) = (corner(a, i, z), corner(a, i + 1, z));
            let (i0, i1) = (corner(b, i, z), corner(b, i + 1, z));
            if up {
                out.push([o0, o1, i1]);
                out.push([o0, i1, i0]);
            } else {
                out.push([o0, i1, o1]);
                out.push([o0, i0, i1]);
            }
        }
    }
    out
}

/// The underside of the cube.
const SURFACE: f32 = -5.0;
/// The lift that every sound file has.
const LIFT: f32 = 5.0;

/// The local z of the build plate for a lift.
fn plate(lift: f32) -> f32 {
    SURFACE - lift
}

fn ball(role: SphereRole, c: V3, r: f32) -> SupportRecord {
    shapes::build(&Spec::Sphere {
        role,
        start: [c[0], c[1], c[2] - r],
        end: [c[0], c[1], c[2] + r],
        radius: r,
    })
    .expect("a native ball")
}

fn tip_cone(s: V3, e: V3, r1: f32, r2: f32) -> SupportRecord {
    shapes::build(&Spec::TipCone {
        start: s,
        end: e,
        r1,
        r2,
        contact: 0.1,
    })
    .expect("a native tip cone")
}

fn frustum(role: FrustumRole, s: V3, e: V3, r1: f32, r2: f32) -> SupportRecord {
    shapes::build(&Spec::Frustum {
        role,
        start: s,
        end: e,
        r1,
        r2,
    })
    .expect("a native frustum")
}

fn rod(s: V3, e: V3, r: f32) -> SupportRecord {
    frustum(FrustumRole::Rod, s, e, r, r)
}

pub(crate) fn pad(x: f32, y: f32, lift: f32) -> SupportRecord {
    shapes::build(&Spec::RaftPad {
        centre: [x, y],
        z_top: plate(lift) + 0.2,
        thickness: 0.2,
        diameter: 2.2,
    })
    .expect("a native raft pad")
}

/// A complete support: tip ball (radius `r`, centre `s` below the surface, so the patch is
/// pi (r^2 - s^2)), tip cone, joint, column and raft pad. The column is plate-rooted.
pub(crate) fn head(x: f32, y: f32, lift: f32, r: f32, s: f32) -> Vec<SupportRecord> {
    let cz = SURFACE - s;
    let joint_z = cz - 1.5;
    vec![
        ball(SphereRole::TipBall, [x, y, cz], r),
        tip_cone([x, y, cz], [x, y, joint_z], (0.8 * r).min(0.3), 0.25),
        ball(SphereRole::JointBall, [x, y, joint_z], 0.26),
        rod([x, y, joint_z], [x, y, plate(lift) + 0.198], 0.25),
        pad(x, y, lift),
    ]
}

/// The tip radius that gives a patch of `area` mm2 at standoff `s`... as (r, s) for radius `r`.
fn standoff_for(r: f32, area: f32) -> f32 {
    (r * r - area / std::f32::consts::PI).sqrt()
}

// ---- cases ------------------------------------------------------------------------------

/// A file under test and the STL it claims to come from.
pub(crate) struct Built {
    pub(crate) mesh: Vec<Triangle>,
    translate: V3,
    pub(crate) records: Vec<SupportRecord>,
    /// The input STL's triangles (world frame); `None` means the model as written.
    stl: Option<Vec<Triangle>>,
}

impl Built {
    /// A file with the given supports and lift. A raft pad far from everything marks the plate.
    fn new(mesh: Vec<Triangle>, lift: f32, mut records: Vec<SupportRecord>) -> Self {
        records.push(pad(9.0, 9.0, lift));
        Self {
            mesh,
            translate: [0.0, 0.0, lift],
            records,
            stl: None,
        }
    }

    fn with_stl(mut self, stl: Vec<Triangle>) -> Self {
        self.stl = Some(stl);
        self
    }

    /// The bytes of the project, written by the production writer.
    pub(crate) fn chitubox(&self) -> Vec<u8> {
        let supports = (!self.records.is_empty()).then(|| Supports {
            presets: vec![Preset {
                id: 0,
                name: "Ultralight".to_owned(),
            }],
            attrs: vec![0; self.records.len()],
            unreferenced_blob: Vec::new(),
            records: self.records.clone(),
        });
        let project = Project {
            preview: Some(Preview::solid(64, 64, BACKGROUND).expect("preview")),
            models: vec![Model {
                name: "broken case".to_owned(),
                translate: self.translate,
                mesh: self.mesh.clone(),
                supports,
            }],
            ..Project::default()
        };
        project.to_bytes().expect("the production writer writes it")
    }

    /// The input STL, binary.
    pub(crate) fn stl_bytes(&self) -> Vec<u8> {
        let tris = self.stl.as_ref().unwrap_or(&self.mesh);
        let mut out = vec![0_u8; 80];
        out.extend_from_slice(&u32::try_from(tris.len()).expect("count").to_le_bytes());
        for t in tris {
            out.extend_from_slice(&[0; 12]);
            for v in t.iter().flatten() {
                out.extend_from_slice(&v.to_le_bytes());
            }
            out.extend_from_slice(&[0; 2]);
        }
        out
    }
}

/// What the rule under test must report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Expect {
    Violation,
    Unverified,
    Pass,
    NotApplicable,
}

impl Expect {
    pub(crate) fn word(self) -> &'static str {
        match self {
            Self::Violation => "violation",
            Self::Unverified => "unverified",
            Self::Pass => "pass",
            Self::NotApplicable => "not_applicable",
        }
    }
}

pub(crate) struct Case {
    pub(crate) rule: &'static str,
    pub(crate) name: &'static str,
    pub(crate) expect: Expect,
    pub(crate) build: fn() -> Built,
}

const fn case(
    rule: &'static str,
    name: &'static str,
    expect: Expect,
    build: fn() -> Built,
) -> Case {
    Case {
        rule,
        name,
        expect,
        build,
    }
}

const V: Expect = Expect::Violation;
const U: Expect = Expect::Unverified;
const P: Expect = Expect::Pass;

fn cube10(records: Vec<SupportRecord>, lift: f32) -> Built {
    Built::new(cube([0.0; 3], 5.0, false), lift, records)
}

/// The sound file: one complete support on the underside, a plate-rooted column.
pub(crate) fn sound() -> Built {
    cube10(head(0.0, 0.0, LIFT, 0.25, 0.05), LIFT)
}

// H1 ------------------------------------------------------------------------------------
// Smallest seen: a written patch of 0.506 mm2 (limit 0.50). The patch is measured on the
// written 16-gon mesh (about 96.3% of the area the fields name; `standoff_for` names the latter).

fn h1_oversized_tip_patch() -> Built {
    // Tip radius 0.45, centre on the surface: pi 0.2025 = 0.636 mm2.
    cube10(head(0.0, 0.0, LIFT, 0.45, 0.0), LIFT)
}

fn h1_patch_0_506() -> Built {
    // The fields name 0.525 mm2; the written mesh holds 0.506 mm2.
    cube10(head(0.0, 0.0, LIFT, 0.45, standoff_for(0.45, 0.525)), LIFT)
}

fn h1_patch_0_49_control() -> Built {
    cube10(head(0.0, 0.0, LIFT, 0.45, standoff_for(0.45, 0.49)), LIFT)
}

fn h1_oversized_foot_on_model() -> Built {
    // A model-to-model foot, radius 1.0, centred on the cube's top face.
    let recs = vec![
        ball(SphereRole::FootBall, [0.0, 0.0, 5.0], 1.0),
        frustum(
            FrustumRole::FootCone,
            [0.0, 0.0, 5.0],
            [0.0, 0.0, 7.0],
            1.0,
            0.5,
        ),
    ];
    cube10(recs, LIFT)
}

fn h1_ball_mesh_missing_faces() -> Built {
    // A tip ball whose mesh lacks one triangle: not the native shape for its fields.
    let mut recs = head(0.0, 0.0, LIFT, 0.25, 0.05);
    recs[0].mesh.pop();
    cube10(recs, LIFT)
}

fn h1_rod_cap_coplanar_with_the_model_face() -> Built {
    // A radius 0.3 rod ends exactly on the cube's underside (a 0.283 mm2 patch, well inside
    // the limit). A coplanar cap cannot be bracketed to 0.010 mm2 here, so it is unverified:
    // never a pass on an interval that wide.
    let z0 = plate(LIFT);
    let mut recs = head(0.0, 0.0, LIFT, 0.25, 0.05);
    recs.push(rod([3.0, 0.0, z0 + 0.198], [3.0, 0.0, SURFACE], 0.3));
    recs.push(pad(3.0, 0.0, LIFT));
    cube10(recs, LIFT)
}

// H2 ------------------------------------------------------------------------------------
// Smallest seen: patches 0.005 mm apart (fused below 0.010).

fn h2_two_patches_of_one_ball_on_a_thin_slab() -> Built {
    // A radius 0.1 tip ball inside a slab 0.005 mm thick touches both faces: two patches of
    // one support, 0.005 mm apart.
    let slab: Vec<Triangle> = cube([0.0; 3], 5.0, false)
        .iter()
        .map(|t| t.map(|v| [v[0], v[1], SURFACE + v[2] * 0.0005]))
        .collect();
    Built::new(
        slab,
        LIFT,
        vec![ball(SphereRole::TipBall, [0.0, 0.0, SURFACE], 0.1)],
    )
}

fn two_heads(x2: f32, r: f32) -> Built {
    let mut recs = head(0.0, 0.0, LIFT, r, 0.0);
    recs.extend(head(x2, 0.0, LIFT, r, 0.0));
    cube10(recs, LIFT)
}

fn h2_overlapping_patches() -> Built {
    two_heads(0.5, 0.3)
}

fn h2_gap_0_005() -> Built {
    two_heads(2.0 * 0.3 + 0.005, 0.3)
}

fn h2_gap_0_012_control() -> Built {
    two_heads(2.0 * 0.3 + 0.012, 0.3)
}

// H3 ------------------------------------------------------------------------------------
// Smallest seen: a brace whose lowest layer is 1 layer (0.05 mm) below anything under it.

fn h3_brace_hanging_in_the_air() -> Built {
    let mut recs = head(0.0, 0.0, LIFT, 0.25, 0.05);
    let z0 = plate(LIFT);
    // Joined to the column at its top end; the other end floats 0.8 mm above the plate.
    recs.push(rod([3.0, 0.0, z0 + 1.0], [0.0, 0.0, z0 + 2.0], 0.15));
    cube10(recs, LIFT)
}

fn h3_brace_rising_from_the_column_control() -> Built {
    let mut recs = head(0.0, 0.0, LIFT, 0.25, 0.05);
    let z0 = plate(LIFT);
    recs.push(rod([0.0, 0.0, z0 + 2.0], [3.0, 0.0, z0 + 4.0], 0.15));
    cube10(recs, LIFT)
}

fn h3_floating_joint_ball() -> Built {
    let mut recs = head(0.0, 0.0, LIFT, 0.25, 0.05);
    recs.push(ball(
        SphereRole::JointBall,
        [4.0, 4.0, plate(LIFT) + 3.0],
        0.3,
    ));
    cube10(recs, LIFT)
}

/// Two coaxial posts, 3.01 mm and 3.03 mm above the plate: an air gap of `0.02` between them
/// (the lower ends at `lower_top`, the upper begins at `upper_bottom`).
fn h3_posts(lower_top: f32, upper_bottom: f32) -> Built {
    let z0 = plate(LIFT);
    let mut recs = head(0.0, 0.0, LIFT, 0.25, 0.05);
    recs.push(rod([6.0, 6.0, z0 + 0.198], [6.0, 6.0, z0 + lower_top], 0.3));
    recs.push(rod(
        [6.0, 6.0, z0 + upper_bottom],
        [6.0, 6.0, z0 + 4.0],
        0.3,
    ));
    recs.push(pad(6.0, 6.0, LIFT));
    cube10(recs, LIFT)
}

fn h3_air_gap_0_02_between_coaxial_posts() -> Built {
    // A plate-rooted post ends at plate + 3.01; a coaxial post begins at plate + 3.03.
    h3_posts(3.01, 3.03)
}

fn h3_end_face_gap_of_5e_5_between_coaxial_posts() -> Built {
    // The upper post begins 5e-5 mm above the top of the lower one: between every sample plane
    // of a layer walk. The start of the upper post stands on nothing.
    h3_posts(3.01, 3.010_05)
}

/// The 5e-5 mm case with a second upper rod 0.1 mm to the side that starts at the same height
/// (`lower_top`/`upper_bottom` as for `h3_posts`). The two upper rods overlap each other, but
/// neither is reached from the plate.
fn h3_posts_with_a_twin(lower_top: f32, upper_bottom: f32) -> Built {
    let z0 = plate(LIFT);
    let mut recs = head(0.0, 0.0, LIFT, 0.25, 0.05);
    recs.push(rod([6.0, 6.0, z0 + 0.198], [6.0, 6.0, z0 + lower_top], 0.3));
    recs.push(rod(
        [6.0, 6.0, z0 + upper_bottom],
        [6.0, 6.0, z0 + 4.0],
        0.3,
    ));
    recs.push(rod(
        [6.1, 6.0, z0 + upper_bottom],
        [6.1, 6.0, z0 + 4.0],
        0.3,
    ));
    recs.push(pad(6.0, 6.0, LIFT));
    cube10(recs, LIFT)
}

fn h3_twin_rods_over_an_end_face_gap() -> Built {
    // Round-5 review: two upper rods that overlap each other and start together, 5e-5 mm above
    // the lower post. Each used to count the other as the support under it.
    h3_posts_with_a_twin(3.01, 3.010_05)
}

fn h3_twin_rods_on_overlapping_posts_control() -> Built {
    // The same twin rods on a lower post they overlap by 0.01 mm: seated by the post.
    h3_posts_with_a_twin(3.01, 3.0)
}

/// A shallow rod (radius 0.052 mm, rising 0.003 mm over 3 mm) from `a` to `b` beside the
/// column of a sound head. Plate-relative heights.
fn h3_shallow_rod(a: V3, b: V3) -> Built {
    let z0 = plate(LIFT);
    let mut recs = head(0.0, 0.0, LIFT, 0.25, 0.05);
    recs.push(rod([a[0], a[1], z0 + a[2]], [b[0], b[1], z0 + b[2]], 0.052));
    cube10(recs, LIFT)
}

fn h3_shallow_rod_printing_before_it_reaches_its_carrier() -> Built {
    // Round-6 review: the rod's lowest 0.001 mm prints a strip 2.5 mm to 3 mm out, in the air
    // beside the column; only a little higher the strip reaches the column. No sample plane of
    // the layer walk sees the strip, and the section where the rod is wide enough to be rooted
    // is held by the column.
    h3_shallow_rod([3.0, 0.0, 3.030], [0.0, 0.0, 3.033])
}

fn h3_rotated_shallow_rod_printing_a_strip_beside_its_carrier() -> Built {
    // Round-6 review 2: the rod, turned pi/16 in the plane, prints a strip wider than a sliver
    // 0.6 mm from the column, from the first height the written mesh prints it.
    h3_shallow_rod([2.942_355_9, 0.585_270_94, 3.030], [0.0, 0.0, 3.0304])
}

fn h3_shallow_rod_over_its_carrier_control() -> Built {
    // The same rod from the column's axis to 0.1 mm out: its whole start lies inside the column.
    h3_shallow_rod([0.0, 0.0, 3.030], [0.1, 0.0, 3.033])
}

fn h3_posts_joined_control() -> Built {
    // The upper post begins 0.01 below the top of the lower one: one connected post.
    h3_posts(3.01, 3.0)
}

fn h3_model_with_no_support_under_it() -> Built {
    // Only the marker pad: the cube hangs 5 mm above the plate with nothing holding its
    // underside. Every support part is continuous; the model island is not held.
    cube10(Vec::new(), LIFT)
}

// H4 ------------------------------------------------------------------------------------
// Smallest seen: a lift of 4.999999 mm (limit 5.0).

fn h4_lift(lift: f32) -> Built {
    cube10(head(0.0, 0.0, lift, 0.25, 0.05), lift)
}

fn h4_lift_4_95() -> Built {
    h4_lift(4.95)
}

fn h4_lift_4_999999() -> Built {
    h4_lift(4.999_999)
}

fn h4_model_on_the_raft() -> Built {
    // The raft pad is raised until it touches the model's underside.
    let mut recs = head(0.0, 0.0, LIFT, 0.25, 0.05);
    recs.push(
        shapes::build(&Spec::RaftPad {
            centre: [3.0, 3.0],
            z_top: SURFACE + 0.1,
            thickness: 0.2,
            diameter: 2.2,
        })
        .expect("pad"),
    );
    cube10(recs, LIFT)
}

// H5 ------------------------------------------------------------------------------------

fn h5_cavity_without_vent() -> Built {
    // A sealed 4 mm cavity inside the 10 mm cube: no drain, no vent.
    let mut mesh = cube([0.0; 3], 5.0, false);
    mesh.extend(cube([0.0; 3], 2.0, true));
    Built::new(mesh, LIFT, head(0.0, 0.0, LIFT, 0.25, 0.05))
}

fn h5_open_shell() -> Built {
    // One face triangle missing: the surface is not closed, so cavities cannot be counted.
    let mut mesh = cube([0.0; 3], 5.0, false);
    mesh.pop();
    Built::new(mesh, LIFT, head(0.0, 0.0, LIFT, 0.25, 0.05))
}

fn h5_tube_bore() -> Built {
    // A 4 x 4 mm square tube with a 2 x 2 mm bore: a through hole the proof does not call a
    // drain bore, and no drain measurement is built.
    let z0 = plate(LIFT);
    Built::new(
        tube(2.0, 1.0, 10.0),
        LIFT,
        vec![
            rod([0.0, 0.0, z0 + 0.198], [0.0, 0.0, 1.0], 0.4),
            pad(0.0, 0.0, LIFT),
        ],
    )
}

// H6 ------------------------------------------------------------------------------------
// Smallest seen: a surface 0.295 mm from the model (limit 0.30).

/// A column beside the cube's +x face whose surface is `gap` from the face.
fn h6_column_at(gap: f32, r: f32) -> Built {
    let x = 5.0 + gap + r;
    let z0 = plate(LIFT);
    let mut recs = head(0.0, 0.0, LIFT, 0.25, 0.05);
    recs.push(rod([x, 0.0, z0 + 0.198], [x, 0.0, 4.0], r));
    recs.push(pad(x, 0.0, LIFT));
    cube10(recs, LIFT)
}

fn h6_gap_0_33_control() -> Built {
    h6_column_at(0.33, 0.25)
}

fn h6_gap_0_28() -> Built {
    h6_column_at(0.28, 0.25)
}

fn h6_gap_0_295() -> Built {
    h6_column_at(0.295, 0.25)
}

fn h6_thick_column_surface_at_0_29() -> Built {
    // The centreline is far (0.69 mm) but the surface of a 0.4 mm radius column is 0.29 off.
    h6_column_at(0.29, 0.4)
}

fn h6_joint_touching_the_model() -> Built {
    let mut recs = head(0.0, 0.0, LIFT, 0.25, 0.05);
    recs.push(ball(SphereRole::JointBall, [3.0, 0.0, SURFACE - 0.1], 0.3));
    cube10(recs, LIFT)
}

fn h6_incidental_touch_3_5_mm_from_the_contact() -> Built {
    // A second column pushed 0.5 mm up into the underside, 3.5 mm from the contact.
    let z0 = plate(LIFT);
    let mut recs = head(0.0, 0.0, LIFT, 0.25, 0.05);
    recs.push(rod([3.5, 0.0, z0 + 0.198], [3.5, 0.0, SURFACE + 0.5], 0.25));
    recs.push(pad(3.5, 0.0, LIFT));
    cube10(recs, LIFT)
}

fn h6_region_longer_than_3_mm() -> Built {
    // The tip cone is 3.5 mm long, so it is not a contact-surrounding region.
    let (x, y, cz) = (0.0, 0.0, SURFACE - 0.05);
    let joint_z = cz - 3.5;
    let recs = vec![
        ball(SphereRole::TipBall, [x, y, cz], 0.25),
        tip_cone([x, y, cz], [x, y, joint_z], 0.2, 0.25),
        ball(SphereRole::JointBall, [x, y, joint_z], 0.26),
        rod([x, y, joint_z], [x, y, plate(LIFT) + 0.198], 0.25),
        pad(x, y, LIFT),
    ];
    cube10(recs, LIFT)
}

/// A tip ball of radius 0.2 whose centre is 0.1 mm outside the cube's +x face (it enters the
/// face by 0.1 mm), with a tip cone from the centre along the face: radius `r0` to `r1` about
/// the line x = 5.1, length 1.5. No joint, no column: the tip cone and ball are the only
/// support parts that can be near the model.
fn h6_cone_along_the_wall(to: V3, r0: f32, r1: f32) -> Built {
    let recs = vec![
        ball(SphereRole::TipBall, [5.1, 0.0, 0.0], 0.2),
        tip_cone([5.1, 0.0, 0.0], to, r0, r1),
    ];
    cube10(recs, LIFT)
}

fn h6_cone_closing_on_the_wall_after_the_ball() -> Built {
    // The cone's surface goes from 0.05 to 0.01 mm from the wall: it comes closer after it
    // leaves the contact ball.
    h6_cone_along_the_wall([5.1, 0.0, 1.5], 0.05, 0.09)
}

/// A closed box from `lo` to `hi`, outward winding.
fn block(lo: V3, hi: V3) -> Vec<Triangle> {
    let c = [
        0.5 * (lo[0] + hi[0]),
        0.5 * (lo[1] + hi[1]),
        0.5 * (lo[2] + hi[2]),
    ];
    let h = [
        0.5 * (hi[0] - lo[0]),
        0.5 * (hi[1] - lo[1]),
        0.5 * (hi[2] - lo[2]),
    ];
    cube([0.0; 3], 1.0, false)
        .into_iter()
        .map(|t| t.map(|v| [c[0] + v[0] * h[0], c[1] + v[1] * h[1], c[2] + v[2] * h[2]]))
        .collect()
}

/// The cube of the frame with a recess in its +x face, `width` mm high from z = 0.55, 0.15 mm
/// deep (three abutting boxes; the faces they share lie behind the recess). `None` is the
/// plain cube.
fn walled_cube(recess_width: Option<f32>) -> Vec<Triangle> {
    let Some(w) = recess_width else {
        return cube([0.0; 3], 5.0, false);
    };
    let top = 0.55 + w;
    let mut m = block([-5.0, -5.0, -5.0], [5.0, 5.0, 0.55]);
    m.extend(block([-5.0, -5.0, 0.55], [4.85, 5.0, top]));
    m.extend(block([-5.0, -5.0, top], [5.0, 5.0, 5.0]));
    m
}

/// The frame's cube with a recess in its +x face as ONE closed shell (a prism along y, 10 mm
/// long, of the profile in x and z), so the file has a closed model: the face is at x = 5, and
/// between z = 0.55 and 0.75 it is recessed to x = 4.85.
fn notched_cube() -> Vec<Triangle> {
    // The profile, counter-clockwise in (x, z), and its triangulation (checked by area).
    let p: [[f32; 2]; 8] = [
        [-5.0, -5.0],
        [5.0, -5.0],
        [5.0, 0.55],
        [4.85, 0.55],
        [4.85, 0.75],
        [5.0, 0.75],
        [5.0, 5.0],
        [-5.0, 5.0],
    ];
    let cap: [[usize; 3]; 6] = [
        [0, 1, 2],
        [0, 2, 3],
        [0, 3, 4],
        [0, 4, 6],
        [4, 5, 6],
        [0, 6, 7],
    ];
    let at = |i: usize, y: f32| [p[i][0], y, p[i][1]];
    // Each triangle is wound to face the direction it must face outward.
    let wind = |t: Triangle, out: V3| -> Triangle {
        let (a, b) = (
            [t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]],
            [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]],
        );
        let n = [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ];
        if n[0] * out[0] + n[1] * out[1] + n[2] * out[2] >= 0.0 {
            t
        } else {
            [t[0], t[2], t[1]]
        }
    };
    let mut m = Vec::new();
    for c in cap {
        m.push(wind(
            [at(c[0], -5.0), at(c[1], -5.0), at(c[2], -5.0)],
            [0.0, -1.0, 0.0],
        ));
        m.push(wind(
            [at(c[0], 5.0), at(c[1], 5.0), at(c[2], 5.0)],
            [0.0, 1.0, 0.0],
        ));
    }
    for i in 0..p.len() {
        let j = (i + 1) % p.len();
        let (dx, dz) = (p[j][0] - p[i][0], p[j][1] - p[i][1]);
        let out = [dz, 0.0, -dx];
        m.push(wind([at(i, -5.0), at(j, -5.0), at(j, 5.0)], out));
        m.push(wind([at(i, -5.0), at(j, 5.0), at(i, 5.0)], out));
    }
    m
}

/// The round-4 case: a canonical radius 0.2 tip ball whose centre is 0.11 mm outside the +x
/// face (it enters the face by 0.09 mm), and a radius 0.06 cone from the centre straight up the
/// face, so its surface runs 0.05 mm from the face. A recess of `recess_width` mm in the face
/// lifts the gap and drops it back; the whole rise and fall lie inside one eighth of the cone,
/// where a slab minimum cannot see them.
fn h6_cone_over_a_recess(recess_width: Option<f32>) -> Built {
    let recs = vec![
        ball(SphereRole::TipBall, [5.11, 0.0, 0.0], 0.2),
        tip_cone([5.11, 0.0, 0.0], [5.11, 0.0, 2.9], 0.06, 0.06),
    ];
    Built::new(walled_cube(recess_width), LIFT, recs)
}

fn h6_recess_hides_a_decrease_inside_one_slab() -> Built {
    // The gap is 0.05 mm, then 0.108 mm over the recess, then 0.05 mm again: a fall of 0.058
    // mm, over the 0.005 mm slack, inside one slab.
    h6_cone_over_a_recess(Some(0.2))
}

/// The round-5 case: a canonical radius 0.5 tip ball whose centre is 0.45 mm outside the +x face
/// (it enters the face by 0.05 mm) and a radius 0.06 cone from the centre straight up the face,
/// so its surface runs 0.39 mm from the face, past the 0.35 mm departure cap. `wall` is the
/// model.
fn h6_cone_far_from_the_wall(wall: Vec<Triangle>) -> Built {
    let recs = vec![
        ball(SphereRole::TipBall, [5.45, 0.0, 0.0], 0.5),
        tip_cone([5.45, 0.0, 0.0], [5.45, 0.0, 2.9], 0.06, 0.06),
    ];
    Built::new(wall, LIFT, recs)
}

fn h6_recessed_wall_above_the_departure_cap_control() -> Built {
    // The gap is 0.39 mm, 0.54 mm over the recess and 0.39 mm again: a fall of 0.15 mm, but all
    // of it above the 0.35 mm cap and never under the 0.32 mm floor: a capped departure.
    h6_cone_far_from_the_wall(notched_cube())
}

fn h6_gap_falls_under_the_floor_after_the_cap() -> Built {
    // The gap is 0.39 mm and a bump 0.10 mm high across the face (z 1.0 to 1.2) takes it to
    // 0.29 mm: past the cap, then under the 0.32 mm floor.
    let mut m = block([-5.0, -5.0, -5.0], [5.0, 5.0, 1.0]);
    m.extend(block([-5.0, -5.0, 1.0], [5.1, 5.0, 1.2]));
    m.extend(block([-5.0, -5.0, 1.2], [5.0, 5.0, 5.0]));
    h6_cone_far_from_the_wall(m)
}

fn h6_cone_parallel_to_the_wall_never_departs() -> Built {
    // The same cone over the plain face: the gap stays 0.05 mm to the end of the region. The
    // gap never decreases, but the end of the region must reach 0.32 mm (design 5.5 b).
    h6_cone_over_a_recess(None)
}

fn h6_cone_at_a_constant_gap_above_the_cap_control() -> Built {
    // The round-5 cone over the plain face: the gap is 0.39 mm all along, past the cap and
    // above the floor at the end of the region.
    h6_cone_far_from_the_wall(cube([0.0; 3], 5.0, false))
}

fn h6_gap_returns_to_0_317_after_the_cap() -> Built {
    // The gap is 0.39 mm, and a bump 0.073 mm high across the face (z 1.0 to 1.2) takes it to
    // 0.317 mm: past the cap, then under the 0.32 mm floor by 0.003 mm (the checker once took
    // 0.005 mm off the floor).
    let mut m = block([-5.0, -5.0, -5.0], [5.0, 5.0, 1.0]);
    m.extend(block([-5.0, -5.0, 1.0], [5.073, 5.0, 1.2]));
    m.extend(block([-5.0, -5.0, 1.2], [5.0, 5.0, 5.0]));
    h6_cone_far_from_the_wall(m)
}

fn h6_cone_leaving_the_wall_control() -> Built {
    // The same ball with a cone that leans away from the wall: the gap only grows.
    h6_cone_along_the_wall([6.6, 0.0, 0.5], 0.05, 0.09)
}

// H7 ------------------------------------------------------------------------------------
// Smallest seen: one vertex moved 2e-5 mm (tolerance 1e-5).

fn world(mesh: &[Triangle], shift: V3) -> Vec<Triangle> {
    mesh.iter()
        .map(|t| t.map(|v| [v[0] + shift[0], v[1] + shift[1], v[2] + shift[2]]))
        .collect()
}

fn h7_with(edit: impl FnOnce(&mut Vec<Triangle>)) -> Built {
    let mut stl = cube([0.0; 3], 5.0, false);
    edit(&mut stl);
    sound().with_stl(stl)
}

fn h7_vertex_moved_0_001() -> Built {
    h7_with(|m| m[3][1][2] += 0.001)
}

fn h7_vertex_moved_2e_5() -> Built {
    h7_with(|m| m[3][1][2] += 2e-5)
}

fn h7_vertex_moved_5e_6_control() -> Built {
    h7_with(|m| m[3][1][2] += 5e-6)
}

fn h7_triangle_dropped() -> Built {
    h7_with(|m| {
        m.pop();
    })
}

fn h7_scale_1_001() -> Built {
    h7_with(|m| {
        for v in m.iter_mut().flatten() {
            *v = v.map(|x| x * 1.001);
        }
    })
}

fn h7_reversed_winding() -> Built {
    h7_with(|m| m[0].swap(1, 2))
}

fn h7_cyclic_rotation_and_order_control() -> Built {
    h7_with(|m| {
        m.reverse();
        for t in m.iter_mut() {
            t.rotate_left(1);
        }
    })
}

fn h7_undeclared_move() -> Built {
    // The STL cube is 3 mm to one side of where the file puts it, with no recorded pose.
    sound().with_stl(world(&cube([0.0; 3], 5.0, false), [3.0, 0.0, 0.0]))
}

// H8 ------------------------------------------------------------------------------------

fn h8_never_opened_in_chitubox() -> Built {
    // A tidy file. No ChituBox record exists for it, so H8 cannot be anything but unverified.
    sound()
}

// H9 ------------------------------------------------------------------------------------
// Smallest seen: a rod 0.099982 mm across flats (limit 0.1).

fn h9_rod(r: f32) -> Built {
    let z0 = plate(LIFT);
    let mut recs = head(0.0, 0.0, LIFT, 0.25, 0.05);
    recs.push(rod([4.0, 4.0, z0 + 0.198], [4.0, 4.0, z0 + 3.0], r));
    recs.push(pad(4.0, 4.0, LIFT));
    cube10(recs, LIFT)
}

fn h9_rod_0_052_control() -> Built {
    h9_rod(0.052)
}

fn h9_rod_0_050() -> Built {
    h9_rod(0.050)
}

fn h9_rod_0_05097() -> Built {
    h9_rod(0.05097)
}

fn h9_rod_0_05099_control() -> Built {
    h9_rod(0.05099)
}

fn h9_neck_0_09() -> Built {
    // A tip ball of radius 0.2 on a coaxial cylinder of radius 0.045: a 0.088 mm neck.
    let (x, y, cz) = (0.0, 0.0, SURFACE - 0.05);
    let joint_z = cz - 1.5;
    let recs = vec![
        ball(SphereRole::TipBall, [x, y, cz], 0.2),
        tip_cone([x, y, cz], [x, y, joint_z], 0.045, 0.045),
        ball(SphereRole::JointBall, [x, y, joint_z], 0.26),
        rod([x, y, joint_z], [x, y, plate(LIFT) + 0.198], 0.25),
        pad(x, y, LIFT),
    ];
    cube10(recs, LIFT)
}

fn h9_ball_and_coaxial_frustum() -> Built {
    // Ball R 0.2 with a coaxial frustum r(t) = 0.012 + 0.2 t from the ball centre: the centre
    // line leaves the ball at width 0.104 (radii), the union's minimum on the mesh is 0.099.
    let (x, y, cz) = (0.0, 0.0, SURFACE - 0.05);
    let joint_z = cz - 1.5;
    let r_end = 0.012 + 0.2 * 1.5;
    let recs = vec![
        ball(SphereRole::TipBall, [x, y, cz], 0.2),
        tip_cone([x, y, cz], [x, y, joint_z], 0.012, r_end),
        ball(SphereRole::JointBall, [x, y, joint_z], 0.4),
        rod([x, y, joint_z], [x, y, plate(LIFT) + 0.198], 0.4),
        pad(x, y, LIFT),
    ];
    cube10(recs, LIFT)
}

/// Two radius 0.3 posts side by side, `offset` apart in x, on overlapping raft pads. Their
/// meshes (16-gons) share a lens whose thickness is `0.6 - offset` (a little more for a
/// polygon's flats).
fn h9_two_posts(offset: f32) -> Built {
    let z0 = plate(LIFT);
    let mut recs = head(0.0, 0.0, LIFT, 0.25, 0.05);
    for x in [6.0, 6.0 + offset] {
        recs.push(rod([x, 6.0, z0 + 0.198], [x, 6.0, z0 + 3.0], 0.3));
        recs.push(pad(x, 6.0, LIFT));
    }
    cube10(recs, LIFT)
}

/// Two coaxial radius 0.3 posts on the plate, the upper one starting `gap_ulps` units in the
/// last place of its start coordinate above the end of the lower one (round-4 review: a
/// growth of both solids by 2e-6 mm made a join of a real end-face gap of 9e-7 mm, and the
/// join was then certified). H9 sees two separate fat parts; H3 asks whether one stands on the other.
fn h9_posts_with_an_end_face_gap(gap_ulps: u32) -> Built {
    let z0 = plate(LIFT);
    let low_top = z0 + 3.0;
    // Negative floats: one step toward zero is one unit in the last place up.
    let high_bottom = f32::from_bits(low_top.to_bits() - gap_ulps);
    let mut recs = head(0.0, 0.0, LIFT, 0.25, 0.05);
    recs.push(rod([6.0, 6.0, z0 + 0.198], [6.0, 6.0, low_top], 0.3));
    recs.push(rod([6.0, 6.0, high_bottom], [6.0, 6.0, z0 + 4.0], 0.3));
    recs.push(pad(6.0, 6.0, LIFT));
    cube10(recs, LIFT)
}

fn h3_end_face_gap_of_one_micron() -> Built {
    // Two units in the last place at z = -7: 9.5e-7 mm, inside the 2e-6 mm in which end faces
    // are taken to meet: not shown held (unverified), not a certain air gap.
    h9_posts_with_an_end_face_gap(2)
}

fn h9_end_faces_meeting_exactly_control() -> Built {
    h9_posts_with_an_end_face_gap(0)
}

fn h9_thin_offset_lens_between_thick_rods() -> Built {
    // Offset 0.59: the two 16-gon rods share a lens about 0.01 mm thick and 0.05 mm high.
    h9_two_posts(0.59)
}

fn h9_deep_overlap_between_thick_rods_control() -> Built {
    // Offset 0.5: a fat join, nothing thin.
    h9_two_posts(0.5)
}

/// A rod record whose written mesh is the native shape drawn at radius `mesh_r` about the
/// vertical line through `xy` while its fields say `field_r`. The checker rebuilds the native
/// mesh of the fields and refuses a record whose mesh differs from it by more than 2 units in
/// the last place, so such a record is "not the native shape" and decides nothing.
fn rod_with_mesh_radius(xy: [f32; 2], z: [f32; 2], field_r: f32, mesh_r: f32) -> SupportRecord {
    let mut r = rod([xy[0], xy[1], z[0]], [xy[0], xy[1], z[1]], field_r);
    let k = mesh_r / field_r;
    for t in &mut r.mesh {
        for v in t.iter_mut() {
            v[0] = xy[0] + (v[0] - xy[0]) * k;
            v[1] = xy[1] + (v[1] - xy[1]) * k;
        }
    }
    r
}

fn h9_mesh_narrower_than_the_fields() -> Built {
    // Fields: radius 0.0509828 (0.1 across flats). Mesh: 1e-5 mm smaller, 0.09998674 across:
    // 1e-5 mm is far over 2 units in the last place (1.9e-6 mm at a coordinate of 10), so the
    // record is refused as not the native mesh of its fields.
    let z0 = plate(LIFT);
    let mut recs = head(0.0, 0.0, LIFT, 0.25, 0.05);
    recs.push(rod_with_mesh_radius(
        [4.0, 4.0],
        [z0 + 0.198, z0 + 3.0],
        0.050_982_8,
        0.050_972_8,
    ));
    recs.push(pad(4.0, 4.0, LIFT));
    cube10(recs, LIFT)
}

fn h9_mesh_wider_than_the_fields_at_a_large_coordinate() -> Built {
    // Fields: radius 0.0499 (0.0998 across the circle). Mesh: radius 0.0511, 0.1 across flats
    // and more. At y = 3000 two units in the last place are 4.9e-4 mm and the radii differ by
    // 1.2e-3 mm: the record is refused, so the wide mesh cannot vouch for the thin fields.
    let z0 = plate(LIFT);
    let mut recs = head(0.0, 0.0, LIFT, 0.25, 0.05);
    recs.push(rod_with_mesh_radius(
        [4.0, 3000.0],
        [z0 + 0.198, z0 + 3.0],
        0.0499,
        0.0511,
    ));
    recs.push(pad(4.0, 3000.0, LIFT));
    cube10(recs, LIFT)
}

// H1 (representation) --------------------------------------------------------------------

/// A radius 0.45 rod whose fields end `field_gap` short of the cube's underside, while its
/// written cap stands `cap_in` inside the cube. The solid the fields name does not reach the
/// model, so it has no patch; the written mesh is cut by the face across its whole 16-gon
/// (0.62 mm2, over the limit).
fn h1_rod_cap(field_gap: f32, cap_in: f32) -> Built {
    let z0 = plate(LIFT);
    let mut recs = head(0.0, 0.0, LIFT, 0.25, 0.05);
    let cap = SURFACE - field_gap;
    let mut r = rod([3.0, 0.0, z0 + 0.198], [3.0, 0.0, cap], 0.45);
    for t in &mut r.mesh {
        for v in t.iter_mut() {
            if v[2] >= cap - 1e-6 {
                v[2] = SURFACE + cap_in;
            }
        }
    }
    recs.push(r);
    recs.push(pad(3.0, 0.0, LIFT));
    cube10(recs, LIFT)
}

fn h1_rod_cap_inside_the_model_by_the_envelope() -> Built {
    // Fields end 8e-6 mm short of the face, the written cap stands 2e-6 mm inside the cube:
    // 1e-5 mm off the fields, over 2 units in the last place (1.9e-6 mm at a coordinate of
    // 10). The record is refused as not the native mesh of its fields. It must not pass.
    h1_rod_cap(0.000_008, 0.000_002)
}

fn h1_rod_cap_inside_the_model_by_the_rounding() -> Built {
    // The same rod with the cap one unit in the last place of z = 5 (4.8e-7 mm) on either side
    // of the face: 9.5e-7 mm off the fields, inside the 2 units (1.9e-6 mm at a coordinate of
    // 10) the canonical mesh allows. The record is valid; the solid of the fields has no patch
    // and the written mesh has a patch over the limit. It must not pass.
    h1_rod_cap(4.8e-7, 4.8e-7)
}

/// Move the vertices of a record that lie at height `from` (within 1e-6 mm) to height `to`.
fn move_ring(mut r: SupportRecord, from: f32, to: f32) -> SupportRecord {
    for t in &mut r.mesh {
        for v in t.iter_mut() {
            if (v[2] - from).abs() <= 1e-6 {
                v[2] = to;
            }
        }
    }
    r
}

/// Two coaxial radius 0.3 posts at y = 3000 whose fields leave an air gap of 0.001 mm
/// (`low_top` to `high_bottom` above the plate) while the written meshes overlap by it: the
/// lower mesh runs up to the upper post's start, the upper mesh down to the lower post's end.
/// At y = 3000 two units in the last place are 4.9e-4 mm, so the meshes are 1e-3 mm off their
/// fields and both records are refused.
fn two_posts_overlapping_in_the_mesh_only() -> Built {
    let z0 = plate(LIFT);
    let (low_top, high_bottom) = (z0 + 3.0, z0 + 3.001);
    let mut recs = head(0.0, 0.0, LIFT, 0.25, 0.05);
    let low = rod([6.0, 3000.0, z0 + 0.198], [6.0, 3000.0, low_top], 0.3);
    let high = rod([6.0, 3000.0, high_bottom], [6.0, 3000.0, z0 + 4.0], 0.3);
    recs.push(move_ring(low, low_top, high_bottom));
    recs.push(move_ring(high, high_bottom, low_top));
    recs.push(pad(6.0, 3000.0, LIFT));
    cube10(recs, LIFT)
}

fn h3_air_gap_hidden_by_overlapping_meshes() -> Built {
    two_posts_overlapping_in_the_mesh_only()
}

fn h9_join_made_by_overlapping_meshes_only() -> Built {
    two_posts_overlapping_in_the_mesh_only()
}

/// A tip cone narrowing from radius 0.45 to 0.05 along a wall, with a contact ball at its start,
/// whose written rings are turned by pi / 16 about its axis against the native ones. The turned
/// polygon stands off the wall by an amount that falls along the cone while the circle of the
/// fields keeps its gap, so the departure test sees nothing on the fields. The record is
/// refused: its vertices are not the native ones.
fn h6_turned_rings_hide_the_departure() -> Built {
    let (s, e) = ([0.455_855_82_f32, 0.0, 0.0], [0.059_539_53_f32, 0.0, 2.9]);
    let mut cone = tip_cone(s, e, 0.45, 0.05);
    let axis = {
        let d = [e[0] - s[0], e[1] - s[1], e[2] - s[2]];
        let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        [d[0] / l, d[1] / l, d[2] / l]
    };
    let (sn, cs) = (std::f32::consts::PI / 16.0).sin_cos();
    for t in &mut cone.mesh {
        for v in t.iter_mut() {
            let x = [v[0] - s[0], v[1] - s[1], v[2] - s[2]];
            let dot = axis[0] * x[0] + axis[1] * x[1] + axis[2] * x[2];
            let cr = [
                axis[1] * x[2] - axis[2] * x[1],
                axis[2] * x[0] - axis[0] * x[2],
                axis[0] * x[1] - axis[1] * x[0],
            ];
            for k in 0..3 {
                v[k] = s[k] + x[k] * cs + cr[k] * sn + axis[k] * dot * (1.0 - cs);
            }
        }
    }
    let recs = vec![ball(SphereRole::TipBall, s, 0.5), cone];
    Built::new(cube([-5.0, 0.0, 0.0], 5.0, false), LIFT, recs)
}

// H6 (one contact patch) -----------------------------------------------------------------

fn h6_contact_ball_touches_two_model_sheets() -> Built {
    // Two closed walls with a 0.25 mm gap between their faces; a tip ball of radius 0.2 centred
    // in the gap touches both. At most one patch is the intended contact.
    let mut mesh = cube([-5.0, 0.0, 0.0], 5.0, false);
    mesh.extend(cube([5.25, 0.0, 0.0], 5.0, false));
    let recs = vec![ball(SphereRole::TipBall, [0.1, 0.0, 0.0], 0.2)];
    Built::new(mesh, LIFT, recs)
}

fn h9_thin_join_between_thick_rods() -> Built {
    // Two 0.25 mm radius posts with a 0.08 mm gap between their caps, joined only by a ball of
    // radius 0.04 (0.08 mm across) that touches both caps. Each rod is thick enough; the
    // load-bearing section between them is not.
    let z0 = plate(LIFT);
    let low_top = z0 + 3.0;
    let recs = vec![
        rod([6.0, 6.0, z0 + 0.198], [6.0, 6.0, low_top], 0.25),
        ball(SphereRole::JointBall, [6.0, 6.0, low_top + 0.04], 0.04),
        rod([6.0, 6.0, low_top + 0.08], [6.0, 6.0, low_top + 2.0], 0.25),
        pad(6.0, 6.0, LIFT),
    ];
    cube10(recs, LIFT)
}

pub(crate) const CASES: &[Case] = &[
    case("H1", "oversized tip patch", V, h1_oversized_tip_patch),
    case("H1", "written patch 0.506 mm2", V, h1_patch_0_506),
    case("H1", "patch 0.49 mm2 (control)", P, h1_patch_0_49_control),
    case(
        "H1",
        "oversized foot on model",
        V,
        h1_oversized_foot_on_model,
    ),
    case(
        "H1",
        "ball mesh missing a face",
        U,
        h1_ball_mesh_missing_faces,
    ),
    case(
        "H1",
        "rod cap coplanar with the model face",
        U,
        h1_rod_cap_coplanar_with_the_model_face,
    ),
    case(
        "H2",
        "two patches of one ball on a thin slab",
        V,
        h2_two_patches_of_one_ball_on_a_thin_slab,
    ),
    case("H2", "overlapping patches", V, h2_overlapping_patches),
    case("H2", "patches 0.005 mm apart", V, h2_gap_0_005),
    case(
        "H2",
        "patches 0.012 mm apart (control)",
        P,
        h2_gap_0_012_control,
    ),
    case(
        "H3",
        "brace hanging in the air",
        V,
        h3_brace_hanging_in_the_air,
    ),
    case(
        "H3",
        "brace rising from the column (control)",
        P,
        h3_brace_rising_from_the_column_control,
    ),
    case("H3", "floating joint ball", V, h3_floating_joint_ball),
    case(
        "H3",
        "0.02 mm air gap between coaxial posts",
        V,
        h3_air_gap_0_02_between_coaxial_posts,
    ),
    case(
        "H3",
        "end-face gap of 5e-5 mm between coaxial posts",
        V,
        h3_end_face_gap_of_5e_5_between_coaxial_posts,
    ),
    case(
        "H3",
        "twin rods over an end-face gap seat each other",
        V,
        h3_twin_rods_over_an_end_face_gap,
    ),
    case(
        "H3",
        "twin rods on an overlapping post (control)",
        P,
        h3_twin_rods_on_overlapping_posts_control,
    ),
    case(
        "H3",
        "shallow rod printing before it reaches its carrier",
        V,
        h3_shallow_rod_printing_before_it_reaches_its_carrier,
    ),
    case(
        "H3",
        "rotated shallow rod printing a strip beside its carrier",
        V,
        h3_rotated_shallow_rod_printing_a_strip_beside_its_carrier,
    ),
    case(
        "H3",
        "shallow rod over its carrier (control)",
        P,
        h3_shallow_rod_over_its_carrier_control,
    ),
    case(
        "H3",
        "posts joined in one layer (control)",
        P,
        h3_posts_joined_control,
    ),
    case(
        "H3",
        "model with no support under it",
        V,
        h3_model_with_no_support_under_it,
    ),
    case("H4", "lift 4.95", V, h4_lift_4_95),
    case("H4", "lift 4.999999", V, h4_lift_4_999999),
    case("H4", "model touches a raft pad", V, h4_model_on_the_raft),
    case("H4", "lift 5.0 (control)", P, sound),
    case("H5", "cavity without vent", V, h5_cavity_without_vent),
    case("H5", "open shell", U, h5_open_shell),
    case("H5", "tube bore", U, h5_tube_bore),
    case("H5", "solid cube (control)", Expect::NotApplicable, sound),
    case("H6", "column 0.28 mm off", V, h6_gap_0_28),
    case("H6", "column 0.295 mm off", V, h6_gap_0_295),
    case(
        "H6",
        "thick column surface 0.29 mm off",
        V,
        h6_thick_column_surface_at_0_29,
    ),
    case(
        "H6",
        "joint touching the model",
        V,
        h6_joint_touching_the_model,
    ),
    case(
        "H6",
        "incidental touch 3.5 mm from the contact",
        V,
        h6_incidental_touch_3_5_mm_from_the_contact,
    ),
    case(
        "H6",
        "tip cone longer than 3 mm",
        V,
        h6_region_longer_than_3_mm,
    ),
    case("H6", "column 0.33 mm off (control)", P, h6_gap_0_33_control),
    case(
        "H6",
        "tip cone closing on the wall after the ball",
        V,
        h6_cone_closing_on_the_wall_after_the_ball,
    ),
    case(
        "H6",
        "tip cone leaving the wall (control)",
        P,
        h6_cone_leaving_the_wall_control,
    ),
    case("H7", "vertex moved 0.001 mm", V, h7_vertex_moved_0_001),
    case("H7", "vertex moved 2e-5 mm", V, h7_vertex_moved_2e_5),
    case("H7", "triangle dropped", V, h7_triangle_dropped),
    case("H7", "scale 1.001", V, h7_scale_1_001),
    case("H7", "reversed winding", V, h7_reversed_winding),
    case("H7", "undeclared move", V, h7_undeclared_move),
    case(
        "H7",
        "vertex moved 5e-6 mm (control)",
        P,
        h7_vertex_moved_5e_6_control,
    ),
    case(
        "H7",
        "cyclic rotation and order (control)",
        P,
        h7_cyclic_rotation_and_order_control,
    ),
    case("H8", "never opened", U, h8_never_opened_in_chitubox),
    case("H9", "rod radius 0.050", V, h9_rod_0_050),
    case("H9", "rod radius 0.05097", V, h9_rod_0_05097),
    case("H9", "neck 0.088 mm", V, h9_neck_0_09),
    case(
        "H9",
        "ball with coaxial frustum",
        V,
        h9_ball_and_coaxial_frustum,
    ),
    case(
        "H9",
        "ball thinner than the limit between thick rods",
        V,
        h9_thin_join_between_thick_rods,
    ),
    case(
        "H9",
        "thick rods joined by a thin offset lens",
        V,
        h9_thin_offset_lens_between_thick_rods,
    ),
    case(
        "H9",
        "mesh narrower than its fields",
        U,
        h9_mesh_narrower_than_the_fields,
    ),
    case(
        "H9",
        "mesh wider than the fields at a large coordinate",
        U,
        h9_mesh_wider_than_the_fields_at_a_large_coordinate,
    ),
    case(
        "H3",
        "air gap in the fields hidden by overlapping meshes",
        U,
        h3_air_gap_hidden_by_overlapping_meshes,
    ),
    case(
        "H9",
        "join made by overlapping meshes only",
        U,
        h9_join_made_by_overlapping_meshes_only,
    ),
    case(
        "H6",
        "turned rings hide the departure",
        U,
        h6_turned_rings_hide_the_departure,
    ),
    case(
        "H1",
        "rod cap inside the model by the rounding",
        U,
        h1_rod_cap_inside_the_model_by_the_rounding,
    ),
    case(
        "H1",
        "rod cap inside the model by the envelope",
        U,
        h1_rod_cap_inside_the_model_by_the_envelope,
    ),
    case(
        "H6",
        "contact ball touches two model sheets",
        V,
        h6_contact_ball_touches_two_model_sheets,
    ),
    case(
        "H6",
        "recess hides a decrease inside one slab",
        V,
        h6_recess_hides_a_decrease_inside_one_slab,
    ),
    case(
        "H6",
        "cone parallel to the wall never departs",
        V,
        h6_cone_parallel_to_the_wall_never_departs,
    ),
    case(
        "H6",
        "cone at a constant 0.39 mm gap (control)",
        P,
        h6_cone_at_a_constant_gap_above_the_cap_control,
    ),
    case(
        "H6",
        "gap returns to 0.317 mm after the cap",
        V,
        h6_gap_returns_to_0_317_after_the_cap,
    ),
    case(
        "H6",
        "recessed wall above the departure cap (control)",
        P,
        h6_recessed_wall_above_the_departure_cap_control,
    ),
    case(
        "H6",
        "gap falls under the floor after the cap",
        V,
        h6_gap_falls_under_the_floor_after_the_cap,
    ),
    case(
        "H3",
        "end-face gap of one micron between rods",
        U,
        h3_end_face_gap_of_one_micron,
    ),
    case(
        "H9",
        "end faces meeting exactly (control)",
        P,
        h9_end_faces_meeting_exactly_control,
    ),
    case(
        "H9",
        "thick rods overlapping deeply (control)",
        P,
        h9_deep_overlap_between_thick_rods_control,
    ),
    case("H9", "rod radius 0.052 (control)", P, h9_rod_0_052_control),
    case(
        "H9",
        "rod radius 0.05099 (control)",
        P,
        h9_rod_0_05099_control,
    ),
];

/// The cases added for the review's false passes (a rule passed on a bound that did not cover
/// what the file holds). Each must also block through the normal CLI (`rule_blocks`).
pub(crate) const REVIEW_CASES: &[(&str, &str)] = &[
    ("H1", "rod cap coplanar with the model face"),
    ("H2", "two patches of one ball on a thin slab"),
    ("H3", "0.02 mm air gap between coaxial posts"),
    ("H3", "model with no support under it"),
    ("H6", "tip cone closing on the wall after the ball"),
    ("H9", "thick rods joined by a thin offset lens"),
    ("H9", "mesh narrower than its fields"),
    ("H9", "mesh wider than the fields at a large coordinate"),
    ("H1", "rod cap inside the model by the envelope"),
    ("H6", "contact ball touches two model sheets"),
    ("H3", "air gap in the fields hidden by overlapping meshes"),
    ("H9", "join made by overlapping meshes only"),
    ("H6", "turned rings hide the departure"),
    ("H1", "rod cap inside the model by the rounding"),
    ("H6", "recess hides a decrease inside one slab"),
    ("H3", "end-face gap of one micron between rods"),
    ("H6", "gap falls under the floor after the cap"),
    ("H3", "end-face gap of 5e-5 mm between coaxial posts"),
    ("H3", "twin rods over an end-face gap seat each other"),
    ("H3", "shallow rod printing before it reaches its carrier"),
    (
        "H3",
        "rotated shallow rod printing a strip beside its carrier",
    ),
    ("H6", "cone parallel to the wall never departs"),
    ("H6", "gap returns to 0.317 mm after the cap"),
];

// ---- running and judging: see run.rs ----------------------------------------------------

mod mesh_verdict;
mod run;
pub(crate) use run::{all_cases, check_bytes, scratch};
