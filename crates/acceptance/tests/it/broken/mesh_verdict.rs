//! Controls that pin the verdict scope: a hard-limit verdict is measured on the written
//! triangles, and the solid the record fields name is information only. Each file below is one
//! the fields alone would judge differently from the triangles; the checker's answer must be
//! the triangles' (valid for ChituBox v1.4.0 if it slices the stored support meshes, pending
//! the H8 differential slice test).

use super::{
    Built, Case, FrustumRole, LIFT, P, U, cube10, frustum, h9_posts_with_an_end_face_gap, head,
    plate, sound, standoff_for,
};

/// Round-8 review: probes at y or z near 3000 mm are not files for the printer (build volume
/// |x| <= 120, |y| <= 70, -1 <= z <= 230 mm in plate coordinates). The sound file, placed
/// 300 mm along y, is refused as a whole: every rule unverified, with the reason named.
fn h9_file_beyond_the_build_volume_in_y() -> Built {
    let mut file = sound();
    file.translate[1] = 300.0;
    file
}

/// The same file lifted 300 mm (the build volume is 220 mm high): refused.
fn h9_file_beyond_the_build_volume_in_z() -> Built {
    let mut file = sound();
    file.translate[2] = 300.0;
    file
}

/// The fields name a contact patch of 0.505 mm2 (over the 0.50 limit); the written 16-gon mesh
/// holds 0.486 mm2. H1 is measured on the mesh: a pass.
fn h1_analytic_patch_over_the_limit_written_patch_under() -> Built {
    cube10(head(0.0, 0.0, LIFT, 0.45, standoff_for(0.45, 0.505)), LIFT)
}

/// Two coaxial posts with a real end-face gap in the written meshes: each is a fat part and
/// they share no join, so H9 passes (whether the upper post stands on the lower is H3's).
fn h9_two_separate_posts() -> Built {
    h9_posts_with_an_end_face_gap(40)
}

/// Round-7 review: a shallow frustum (radius 0.2 at the column's axis, 0.5 at its far end, 3 mm
/// out and 0.2955 mm up). The solid the fields name first appears 0.0069 mm lower than the
/// written mesh does, at the far end. The written mesh first prints at the near end, inside the
/// column: H3 is measured on it, a pass, and the difference is reported as information.
fn h3_shallow_tapered_frustum_beside_its_column() -> Built {
    let z0 = plate(LIFT);
    let mut recs = head(0.0, 0.0, LIFT, 0.25, 0.05);
    recs.push(frustum(
        FrustumRole::Rod,
        [0.0, 0.0, z0 + 3.03],
        [2.942_355_6, 0.585_271, z0 + 3.325_511_5],
        0.2,
        0.5,
    ));
    cube10(recs, LIFT)
}

pub(crate) const CASES: &[Case] = &[
    Case {
        rule: "H9",
        name: "file 300 mm beyond the build volume in y is refused",
        expect: U,
        build: h9_file_beyond_the_build_volume_in_y,
    },
    Case {
        rule: "H9",
        name: "file 300 mm above the plate is refused",
        expect: U,
        build: h9_file_beyond_the_build_volume_in_z,
    },
    Case {
        rule: "H1",
        name: "analytic patch over the limit, written patch under it (control)",
        expect: P,
        build: h1_analytic_patch_over_the_limit_written_patch_under,
    },
    Case {
        rule: "H9",
        name: "posts across an end-face gap are two separate parts (control)",
        expect: P,
        build: h9_two_separate_posts,
    },
    Case {
        rule: "H3",
        name: "shallow tapered frustum beside its column (control)",
        expect: P,
        build: h3_shallow_tapered_frustum_beside_its_column,
    },
];
